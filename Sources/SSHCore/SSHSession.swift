import NIOConcurrencyHelpers
import NIOCore
import NIOPosix
import NIOSSH

/// En SSH-anslutning. Anslut, kör kommandon (strömmad utdata), stäng.
/// Byggd rakt på SwiftNIO SSH — samma kod på Linux och Apple.
public final class SSHSession {
    private let target: SSHTarget
    private let auth: SSHAuth
    private let knownHosts: KnownHosts
    // internal (inte private) — PortForward.swifts SSHSession-extension i
    // samma modul behöver nå dem.
    let group: MultiThreadedEventLoopGroup
    // Bär ett fatalt fel som inträffar tyst under handshaken (NIOSSH stänger inte
    // alltid anslutningen vid misslyckad auth eller avvisad värdnyckel). När den
    // fullbordas avslutas pågående strömmar i stället för att hänga för evigt.
    private let fatal: EventLoopPromise<Error>
    private let fatalLock = NIOLock()
    private var fatalResolved = false
    var channel: Channel?

    // internal (inte private) — `SFTPClient.open(on:)` racear sitt eget
    // child-kanalskapande mot samma `fatal`-signal som `execute()`/
    // `openShell()` redan gör, se kommentaren vid `SFTPClient.open`.
    var fatalFuture: EventLoopFuture<Error> { fatal.futureResult }

    // Fjärr-portvidarebefordran (ssh -R): servern öppnar en "forwarded-tcpip"-
    // kanal MOT klienten när någon ansluter till den fjärrport som begärts.
    // inboundChildChannelInitializer sätts en gång vid connect() (innan någon
    // openRemotePortForward()-anrop kan ha hunnit ske), så routningen måste gå
    // via en delad, trådsäker tabell istället för att fångas direkt i closuren.
    // Keyad enbart på port (inte host) — tillräckligt för det vanliga fallet
    // av en vidarebefordran per port, se PortForward.swift.
    let remoteForwards = NIOLockedValueBox<[Int: (targetHost: String, targetPort: Int)]>([:])

    public init(target: SSHTarget, auth: SSHAuth, knownHosts: KnownHosts = KnownHosts()) {
        self.target = target
        self.auth = auth
        self.knownHosts = knownHosts
        self.group = MultiThreadedEventLoopGroup(numberOfThreads: 1)
        self.fatal = group.next().makePromise(of: Error.self)
    }

    // fatal får bara fullbordas en gång (annars kraschar NIO). Första signalen
    // vinner — auth-give-up, avvisad värdnyckel eller close().
    private func signalFatal(_ error: Error) {
        let firstToResolve: Bool = fatalLock.withLock {
            if fatalResolved { return false }
            fatalResolved = true
            return true
        }
        guard firstToResolve else { return }
        fatal.succeed(error)
        // NIOSSH stänger INTE kanalen självt vid ett avvisat auth-/värdnyckel-
        // handslag (se `TOFUHostKeyValidator`s kommentar) — SSH-handskakningen
        // abortas internt, men den råa TCP-kanalen förblir öppen och
        // `channel.closeFuture` triggas ALDRIG. Ett `createChannel(...)`-anrop
        // (execute()/openShell()) som redan hunnit starta MOT den kanalen får
        // därför sitt promise föräldralöst för alltid — NIOSSHs multiplexer
        // svarar aldrig, varken lyckat eller misslyckat, eftersom den aldrig
        // nådde "auktoriserad"-läget. Utan att stänga kanalen HÄR hänger
        // `waitForChildOpsToDrain()` (och därmed `close()`) i sin 15s-timeout
        // varje gång — bevisat 100 % reproducerbart på riktig macOS-hårdvara
        // 2026-07-20 via ett fristående repro-program (KnownHostsTests.
        // testChangedHostKeyRejected). Att stänga kanalen här får NIOSSHs
        // egen kanalstädning att köra, vilket i sin tur får den föräldralösa
        // createChannel-promisen att faktiskt misslyckas (via dess egen
        // closeFuture-koppling i NIOSSHHandler) i stället för att aldrig
        // röras.
        channel?.close(promise: nil)
    }

    // close() anropar signalFatal(...) synkront som sitt FÖRSTA steg, innan
    // den asynkrona nedstängningen (channel.close()/group.shutdownGracefully())
    // ens börjar. Den här flaggan låter openShell() (och annat som gör en
    // .get()-blockerande pipeline-uppslagning, till skillnad från execute()s
    // callback-baserade mönster som redan race:ar säkert mot closeFuture/fatal)
    // upptäcka en pågående/redan avslutad close() INNAN den rör en kanal vars
    // event loop-grupp kan hinna stängas ner under tiden — annars kraschar
    // processen med NIOs "leaking promise"-fatal error istället för att kasta
    // ett vanligt Swift-fel (reproducerat i TerminalTeardownRaceTests).
    private var isClosingOrClosed: Bool { fatalLock.withLock { fatalResolved } }

    // Den KVARVARANDE racet efter a413e25 (#169): den fixen skyddar bara
    // SJÄLVA pipeline-uppslagningen (channel.pipeline.handler(type:).whenComplete)
    // mot close() — men INTE fönstret DÄREFTER, mellan att sshHandler.
    // createChannel(childPromise, ...) anropas och att childPromise faktiskt
    // löses (kräver en fram-och-tillbaka SSH-kanalöppning, inte omedelbar).
    // Om close() kör channel.close() + group.shutdownGracefully() medan
    // childPromise fortfarande är olöst på den event loop-gruppen kraschar
    // NIOs läckagedetektor ("leaking promise") — reproducerat på macOS-CI.
    // Löst genom att close() STÄNGER INPASSERINGEN (isClosed) ATOMÄRT innan
    // den dränerar räknaren, och att beginChildOp() kastar om sessionen
    // redan är stängd — garanterar att inga nya operationer kan börja efter
    // att close() börjat dränera, så att dränering faktiskt innebär "inga
    // fler operationer kommer någonsin".
    private let inFlightChildOps = NIOLockedValueBox<Int>(0)
    private let drainLock = NIOLock()
    private var isClosed = false
    private var onDrainedCallbacks: [() -> Void] = []

    // internal (inte private) — `SFTPClient.open(on:)` i samma modul
    // registrerar sin egen kanalöppning som en barn-operation här, se
    // kommentaren vid `SFTPClient.open`.
    func beginChildOp() throws {
        try drainLock.withLock {
            guard !isClosed else {
                throw SSHError.channelFailed("stängd")
            }
            inFlightChildOps.withLockedValue { $0 += 1 }
        }
    }

    /// Anropas när ETT specifikt childPromise-resultat (oavsett lyckat/
    /// misslyckat) blivit klart — ren observation via `.whenComplete`,
    /// rör ALDRIG promisens eget resultat (det gör bara NIOSSH:s interna
    /// createChannel-logik, för att undvika en "dubbelt fullbordad
    /// promise"-krasch av samma familj som det vi fixar här).
    func endChildOp() {
        let remaining: Int = inFlightChildOps.withLockedValue { count in
            count -= 1
            return count
        }
        guard remaining == 0 else { return }
        let callbacks = drainLock.withLock {
            let result = onDrainedCallbacks
            onDrainedCallbacks = []
            return result
        }
        for callback in callbacks {
            callback()
        }
    }

    /// Väntar tills `inFlightChildOps` når noll. Använder OSTRUKTURERADE
    /// `Task`er (inte en `TaskGroup`) för timeouten — en TaskGroup väntar
    /// in ALLA barn-tasks innan den returnerar även efter `cancelAll()` om
    /// en av dem hänger i en `withCheckedContinuation` som aldrig resumeras
    /// av cancellation (samma TaskGroup-fallgrop som dokumenterats i
    /// Tests/SSHCoreTests/TerminalTeardownRaceTests.swift i `withTimeout`).
    /// Timeout (15s) som sista utväg om ett childPromise aldrig löses (t.ex.
    /// nätverksavbrott utan NIOSSH-timeout) — men teardown tillåts ENDAST om
    /// räknaren faktiskt når noll, även efter timeouten, för att bevara
    /// säkerhetsinvarianten.
    private func waitForChildOpsToDrain() async {
        if drainLock.withLock({ isClosed && inFlightChildOps.withLockedValue({ $0 == 0 }) }) {
            return
        }
        await withCheckedContinuation { (cont: CheckedContinuation<Void, Never>) in
            var didResume = false
            let resumeLock = NIOLock()
            func resumeOnce() {
                let shouldRun: Bool = resumeLock.withLock {
                    if didResume { return false }
                    didResume = true
                    return true
                }
                if shouldRun { cont.resume() }
            }

            let alreadyDrained: Bool = drainLock.withLock {
                if inFlightChildOps.withLockedValue({ $0 == 0 }) { return true }
                onDrainedCallbacks.append(resumeOnce)
                return false
            }
            if alreadyDrained { resumeOnce(); return }

            Task {
                try? await Task.sleep(nanoseconds: 15_000_000_000)
                let remaining = drainLock.withLock {
                    inFlightChildOps.withLockedValue { $0 }
                }
                guard remaining > 0 else {
                    resumeOnce()
                    return
                }
                // Inte längre den kända NIOSSH-host-key-rejection-buggen
                // (den stänger redan kanalen omedelbart via signalFatal()) —
                // men skydda ändå mot en GENUINT långsam createChannel-
                // operation (t.ex. ett riktigt, förlustfyllt nätverk, till
                // skillnad från loopback-testerna) genom att aktivt PRÖVA
                // den bevisade åtgärden i stället för att bara ge upp:
                // stänga kanalen igen (idempotent om redan stängd) — det
                // är precis det som får NIOSSH att fela en föräldralös
                // promise i stället för att aldrig röra den (se
                // signalFatal()). En kort nådatid ger `endChildOp()` en
                // chans att faktiskt köra innan vi (cubic P1 på PR #183:
                // annars kringgår detta dräneringsinvarianten och kan
                // riva event loop-gruppen med en promise fortfarande olöst).
                channel?.close(promise: nil)
                try? await Task.sleep(nanoseconds: 3_000_000_000)
                let stillRemaining = drainLock.withLock {
                    inFlightChildOps.withLockedValue { $0 }
                }
                // `assertionFailure` no-opar i release-bygge — att returnera
                // HÄR utan att köra `resumeOnce()` hade då hängt `close()`
                // FÖR ALLTID i produktion om den extra kanalstängningen ändå
                // inte hjälpte, i stället för att krascha som i debug.
                // `resumeOnce()` körs därför OVILLKORLIGT som sista utväg —
                // assertionen är bara ett debug-larm, ingen spärr.
                if stillRemaining > 0 {
                    assertionFailure("waitForChildOpsToDrain: timeout med \(stillRemaining) kvarvarande ops efter aktiv kanalstängning — potentiell hängning")
                }
                resumeOnce()
            }
        }
    }

    /// Öppnar TCP + SSH-handshake + autentisering.
    public func connect() async throws {
        let userAuth = SSHUserAuth(username: target.username, auth: auth) { [weak self] in
            self?.signalFatal(SSHError.authenticationFailed)
        }
        let validator = TOFUHostKeyValidator(
            host: target.host, port: target.port, store: knownHosts
        ) { [weak self] info in
            self?.signalFatal(SSHError.hostKeyRejected(info))
        }
        let bootstrap = ClientBootstrap(group: group)
            .channelInitializer { [weak self] channel in
                channel.pipeline.addHandlers([
                    NIOSSHHandler(
                        role: .client(.init(
                            userAuthDelegate: userAuth,
                            serverAuthDelegate: validator)),
                        allocator: channel.allocator,
                        inboundChildChannelInitializer: { inboundChannel, channelType in
                            self?.handleInboundForwardedChannel(inboundChannel, channelType: channelType)
                                ?? inboundChannel.eventLoop.makeFailedFuture(
                                    SSHError.channelFailed("sessionen är borta"))
                        })
                ])
            }
            .channelOption(ChannelOptions.socketOption(.so_reuseaddr), value: 1)

        do {
            channel = try await bootstrap.connect(host: target.host, port: target.port).get()
        } catch {
            throw SSHError.connectionFailed(String(describing: error))
        }
    }

    /// Ansluter GENOM en redan uppkopplad session (`ssh -J`/ProxyJump) —
    /// istället för en ny TCP-anslutning öppnas en `direct-tcpip`-kanal från
    /// `jump` till DEN HÄR sessionens mål, och en helt egen, oberoende SSH-
    /// handskakning (egen `NIOSSHHandler`, eget `SSHUserAuth`/TOFU) körs
    /// direkt ovanpå den kanalen — samma "SSH i SSH"-mönster som en riktig
    /// `ssh -J` gör på trådnivå. `jump` måste redan vara ansluten.
    ///
    /// **Viktigt om stängningsordning**: den här sessionens kanal lever på
    /// `jump`s event loop-grupp (inte sin egen — `self.group` skapas i
    /// `init()` men används aldrig här, precis som vanligt är det ofarligt
    /// att stänga). Stäng DÄRFÖR alltid den här sessionen INNAN `jump`
    /// stängs — tvärtom (stänga `jump` medan den här sessionen fortfarande
    /// är öppen) gör att `jump`s event loop-grupp redan är nedstängd när den
    /// här sessionens `close()` försöker schemalägga sin egen kanalstängning
    /// på den, vilket hänger/misslyckas tyst (bevisat empiriskt: en tidig
    /// testversion med fel ordning hängde processen). Motsvarar för övrigt
    /// exakt hur en riktig `ssh -J` fungerar — dör tunneln (jump), dör allt
    /// som går genom den.
    public func connect(via jump: SSHSession) async throws {
        guard let jumpChannel = jump.channel else {
            throw SSHError.channelFailed("jump-sessionen är inte ansluten")
        }
        let jumpSSHHandler: NIOSSHHandler
        do {
            jumpSSHHandler = try await jumpChannel.pipeline.handler(type: NIOSSHHandler.self).get()
        } catch {
            throw SSHError.channelFailed(String(describing: error))
        }
        guard let originatorAddress = jumpChannel.localAddress else {
            throw SSHError.channelFailed("okänd lokal adress på jump-sessionen")
        }

        let userAuth = SSHUserAuth(username: target.username, auth: auth) { [weak self] in
            self?.signalFatal(SSHError.authenticationFailed)
        }
        let validator = TOFUHostKeyValidator(
            host: target.host, port: target.port, store: knownHosts
        ) { [weak self] info in
            self?.signalFatal(SSHError.hostKeyRejected(info))
        }

        let directTCPIP = SSHChannelType.DirectTCPIP(
            targetHost: target.host, targetPort: target.port, originatorAddress: originatorAddress)
        let childPromise = jumpChannel.eventLoop.makePromise(of: Channel.self)
        jumpSSHHandler.createChannel(childPromise, channelType: .directTCPIP(directTCPIP)) { [weak self] child, channelType in
            guard case .directTCPIP = channelType else {
                return jumpChannel.eventLoop.makeFailedFuture(SSHError.channelFailed("fel kanaltyp"))
            }
            return child.eventLoop.makeCompletedFuture {
                try child.pipeline.syncOperations.addHandler(DirectTCPIPWrapperHandler())
                try child.pipeline.syncOperations.addHandler(NIOSSHHandler(
                    role: .client(.init(userAuthDelegate: userAuth, serverAuthDelegate: validator)),
                    allocator: child.allocator,
                    inboundChildChannelInitializer: { inboundChannel, channelType in
                        self?.handleInboundForwardedChannel(inboundChannel, channelType: channelType)
                            ?? inboundChannel.eventLoop.makeFailedFuture(
                                SSHError.channelFailed("sessionen är borta"))
                    }))
            }
        }
        do {
            channel = try await childPromise.futureResult.get()
        } catch {
            throw SSHError.connectionFailed(String(describing: error))
        }
    }

    /// Kör ett kommando och strömmar stdout/stderr allteftersom det kommer.
    /// Kastar `SSHError.remoteExit` om exitkoden är != 0.
    public func execute(_ command: String) -> AsyncThrowingStream<SSHChunk, Error> {
        AsyncThrowingStream { continuation in
            do {
                try self.beginChildOp()
            } catch {
                continuation.finish(throwing: error)
                return
            }
            guard let channel = self.channel else {
                self.endChildOp()
                continuation.finish(throwing: SSHError.channelFailed("inte ansluten"))
                return
            }

            // Samma "vinnaren avgör"-mönster som openShell() (se
            // resolveOnce där) — en gemensam, lås-skyddad flagga garanterar
            // att createChannel() för exec-kanalen ALDRIG anropas om
            // anslutningen redan dött (fatal/closeFuture vann racet) INNAN
            // pipeline-uppslagningen hann svara. Utan detta kunde en tyst
            // handskakningsdöd (t.ex. en avvisad värdnyckel, som gör att
            // NIOSSH aldrig etablerar någon multiplexer) ändå låta
            // pipeline-uppslagningen lyckas och anropa createChannel() i
            // alla fall — den promisen löses då ALDRIG (ingen fungerande
            // mux att öppna en kanal genom), och endChildOp() (bunden till
            // just den promisen) skulle aldrig köra. Det hänger close() i
            // 15s tills timeout-assertionen kraschar — bevisat 100 %
            // reproducerbart på riktig macOS-hårdvara 2026-07-20 via
            // KnownHostsTests.testChangedHostKeyRejected (samma
            // sårbarhetsklass som en cubic-autofix på PR #183 råkade
            // återinföra genom att flytta endChildOp() till just den
            // opskyddade promisen).
            let resolveLock = NIOLock()
            var resolved = false
            func resolveOnce(_ body: () -> Void) {
                let shouldRun: Bool = resolveLock.withLock {
                    if resolved { return false }
                    resolved = true
                    return true
                }
                if shouldRun { body() }
            }

            channel.closeFuture.whenComplete { _ in
                resolveOnce {
                    self.endChildOp()
                    continuation.finish(throwing: SSHError.channelFailed("anslutningen stängdes"))
                }
            }
            self.fatal.futureResult.whenSuccess { error in
                resolveOnce {
                    self.endChildOp()
                    continuation.finish(throwing: error)
                }
            }
            channel.pipeline.handler(type: NIOSSHHandler.self).whenComplete { result in
                resolveOnce {
                    switch result {
                    case .failure(let e):
                        self.endChildOp()
                        continuation.finish(throwing: SSHError.channelFailed(String(describing: e)))
                    case .success(let sshHandler):
                        let promise = channel.eventLoop.makePromise(of: Channel.self)
                        promise.futureResult.whenComplete { _ in self.endChildOp() }
                        sshHandler.createChannel(promise, channelType: .session) { child, _ in
                            child.pipeline.addHandler(
                                ExecHandler(command: command, continuation: continuation))
                        }
                        promise.futureResult.whenFailure { e in
                            continuation.finish(throwing: SSHError.channelFailed(String(describing: e)))
                        }
                    }
                }
            }
        }
    }

    /// Öppnar en interaktiv shell med PTY. Returnerar ett handtag att skriva
    /// tangenttryck till och läsa strömmad utdata ifrån — det som driver en
    /// terminalvy.
    public func openShell(
        term: String = "xterm-256color", cols: Int = 80, rows: Int = 24
    ) async throws -> SSHShell {
        guard let channel = self.channel, !isClosingOrClosed else {
            throw SSHError.channelFailed("inte ansluten")
        }
        try beginChildOp()

        var cont: AsyncThrowingStream<SSHChunk, Error>.Continuation!
        let stream = AsyncThrowingStream<SSHChunk, Error> { cont = $0 }
        let continuation = cont!

        // Om anslutningen stängs eller handshaken fallerar avslutas shell-strömmen.
        channel.closeFuture.whenComplete { _ in continuation.finish() }
        self.fatal.futureResult.whenSuccess { continuation.finish(throwing: $0) }

        // isClosingOrClosed ovan är bara en preflight-koll (TOCTOU) — en
        // close() kan fortfarande starta EFTER den men INNAN
        // pipeline-uppslagningen nedan hinner svara, vilket river ner
        // event loop-gruppen medan ett promise på den fortfarande är
        // öppet (NIOs "leaking promise"-krasch). Precis som execute()
        // löser vi det genom att registrera closeFuture/fatal-lyssnare
        // som race:ar mot pipeline-uppslagningen via callbacks istället
        // för en naken `try await ...get()`, och låta EN gemensam,
        // låsskyddad flagga avgöra vem som får fullborda resultatet.
        let resultPromise = channel.eventLoop.makePromise(of: SSHShell.self)
        // Motsvarande endChildOp() till beginChildOp() ovan — resultPromise
        // fullbordas garanterat EXAKT en gång (resolveOnce nedan), oavsett
        // vilken väg som vinner, så den här körs också exakt en gång.
        resultPromise.futureResult.whenComplete { _ in self.endChildOp() }
        let resolveLock = NIOLock()
        var resolved = false
        func resolveOnce(_ body: () -> Void) {
            let shouldRun: Bool = resolveLock.withLock {
                if resolved { return false }
                resolved = true
                return true
            }
            if shouldRun { body() }
        }

        self.fatal.futureResult.whenSuccess { error in
            resolveOnce { resultPromise.fail(error) }
        }
        channel.closeFuture.whenComplete { _ in
            resolveOnce { resultPromise.fail(SSHError.channelFailed("inte ansluten")) }
        }
        channel.pipeline.handler(type: NIOSSHHandler.self).whenComplete { result in
            resolveOnce {
                switch result {
                case .failure(let e):
                    resultPromise.fail(SSHError.channelFailed(String(describing: e)))
                case .success(let sshHandler):
                    let handler = ShellHandler(term: term, cols: cols, rows: rows, continuation: continuation)
                    let childPromise = channel.eventLoop.makePromise(of: Channel.self)
                    sshHandler.createChannel(childPromise, channelType: .session) { child, _ in
                        child.pipeline.addHandler(handler)
                    }
                    // Redan "vunnet" ovan (resolved == true) — den här
                    // fullbordar samma, redan reserverade resultat, inte
                    // en ny tävling.
                    childPromise.futureResult.whenComplete { childResult in
                        switch childResult {
                        case .failure(let e):
                            resultPromise.fail(SSHError.channelFailed(String(describing: e)))
                        case .success(let child):
                            resultPromise.succeed(SSHShell(channel: child, output: stream, cols: cols, rows: rows))
                        }
                    }
                }
            }
        }

        return try await resultPromise.futureResult.get()
    }

    /// Bekvämlighet: samla hela utdatan till en sträng.
    public func run(_ command: String) async throws -> String {
        var out = ""
        for try await chunk in execute(command) {
            out += chunk.text
        }
        return out
    }

    public func close() async {
        signalFatal(SSHError.channelFailed("stängd"))
        // Stänger inpasseringen ATOMÄRT (isClosed = true) innan dränering
        // börjar — garanterar att inga nya operationer kan börja efter denna
        // punkt, så att dräneringen faktiskt innebär "inga fler operationer
        // kommer någonsin" (P0-fixen från cubic ultrareview).
        drainLock.withLock {
            isClosed = true
        }
        // Väntar in eventuella pågående createChannel-anrop (openShell()s
        // childPromise) INNAN kanalen/event loop-gruppen rivs — den faktiska
        // fixen för CI-racet, se waitForChildOpsToDrain() ovan.
        await waitForChildOpsToDrain()
        try? await channel?.close().get()
        try? await group.shutdownGracefully()
    }
}

/// En anslutningskedja: en target-session, ev. GENOM en jump-session
/// (`ssh -J`/ProxyJump). Håller ihop de två så att anroparen (App-lagret,
/// t.ex. `SSHTerminalController`) aldrig själv behöver komma ihåg den
/// dokumenterade stängningsordningen (target FÖRE jump, se doc-kommentaren
/// på `SSHSession.connect(via:)` ovan).
public final class SSHConnectionChain {
    public let target: SSHSession
    /// Jump-sessionen, om en användes. INTE `public` (bara `@testable`
    /// synlig) — annars kunde en konsument stänga den FÖRE `target` (via
    /// `chain.jump?.close()`) och bryta den dokumenterade ordningen som
    /// `close()` nedan garanterar. Använd alltid `chain.close()`.
    let jump: SSHSession?

    private init(target: SSHSession, jump: SSHSession?) {
        self.target = target
        self.jump = jump
    }

    /// Ansluter `target` direkt om `jump` är `nil`. Annars ansluts `jump`
    /// FÖRST, och `target` kopplas GENOM den (`connect(via:)`). Endast ETT
    /// hopp stöds — `jump` kan inte i sin tur ha en egen jump-host (se
    /// `HostEditView.jumpCandidates`, som utesluter sådana kandidater i UI:t
    /// tills en fullständig kedjeupplösning finns).
    ///
    /// Fel som kastas INNAN metoden returnerar stänger alla sessioner som
    /// redan hunnit skapas/anslutas. Asynkrona handskakningsfel som syns
    /// först vid `run()`/`openShell()` (efter att `connect(...)` redan
    /// returnerat en kedja) måste däremot städas av ANROPAREN med
    /// `chain.close()` — se `SSHTerminalController.start()` i App-lagret för
    /// ett exempel.
    public static func connect(
        target targetEndpoint: SSHTarget,
        targetAuth: SSHAuth,
        jump jumpEndpoint: (target: SSHTarget, auth: SSHAuth)?,
        knownHosts: KnownHosts = KnownHosts()
    ) async throws -> SSHConnectionChain {
        let targetSession = SSHSession(target: targetEndpoint, auth: targetAuth, knownHosts: knownHosts)
        guard let jumpEndpoint else {
            do {
                try await targetSession.connect()
            } catch {
                await targetSession.close()
                throw error
            }
            return SSHConnectionChain(target: targetSession, jump: nil)
        }
        let jumpSession = SSHSession(target: jumpEndpoint.target, auth: jumpEndpoint.auth, knownHosts: knownHosts)
        do {
            try await jumpSession.connect()
        } catch {
            // targetSession hann aldrig ansluta/skapa en kanal, men dess egen
            // "fatal"-promise (skapad redan i init()) måste ändå upplösas.
            await targetSession.close()
            await jumpSession.close()
            throw error
        }
        do {
            try await targetSession.connect(via: jumpSession)
        } catch {
            // Stäng target FÖRE jump (samma dokumenterade ordning som close()
            // nedan) — annars läcker targetSessions egen "fatal"-promise (se
            // testConnectViaUnconnectedJumpThrows för samma resonemang).
            await targetSession.close()
            await jumpSession.close()
            throw error
        }
        return SSHConnectionChain(target: targetSession, jump: jumpSession)
    }

    /// Stänger i rätt ordning: target FÖRE jump.
    public func close() async {
        await target.close()
        await jump?.close()
    }
}
