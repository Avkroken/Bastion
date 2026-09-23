import XCTest
@testable import SSHCore

// Regressionstest för PR #155 (dubbel-bild-korruption i terminalvyn vid
// dismiss) och dess uppföljning (f8d13dd, "isStopped"-racefixen). Den
// UI-bundna `SSHTerminalController` (App/TerminalView.swift) är
// Xcode-only och kan inte köras här, men hela dess korrekthet vilar på
// ett enda löfte från SSHCore-lagret: att `SSHSession.close()`/
// `SSHShell.close()` verkligen avslutar `shell.output`-strömmen inom
// rimlig tid, oavsett NÄR under sessionens livscykel den anropas. Det är
// exakt det löftet dessa tester bevisar.
/// Kastas av `withTimeout` när operationen inte hann klart — ett tydligt,
/// namngivet fel istället för att bara låta testet hänga tills CI:ts egen
/// (mycket längre) jobb-timeout till slut ger upp.
struct TestTimeoutError: Error, CustomStringConvertible {
    let seconds: Double
    var description: String { "operationen tog längre än \(seconds)s — misstänkt hängning" }
}

/// Löser en enda gång — vinnaren av kapplöpningen mellan `operation` och
/// timeouten i `withTimeout` nedan. Ren `NSLock`, inte en `actor`: att sätta
/// continuation:en måste hända SYNKRONT innan någon av de två racande
/// `Task`:erna ens kan starta köra (se `withCheckedThrowingContinuation`s
/// body nedan), en `actor`s async-isolering skulle själv införa ett fönster
/// där racet kunde köras klart innan continuation:en är satt.
private final class TimeoutRace<T>: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<T, Error>?
    private var finished = false

    func start(_ continuation: CheckedContinuation<T, Error>) {
        lock.lock()
        self.continuation = continuation
        lock.unlock()
    }

    func resume(with result: Result<T, Error>) {
        lock.lock()
        guard !finished, let continuation else { lock.unlock(); return }
        finished = true
        self.continuation = nil
        lock.unlock()
        continuation.resume(with: result)
    }
}

/// Kör `operation` men ger upp (kastar `TestTimeoutError`) om den inte
/// hunnit klart inom `seconds`.
///
/// **VIKTIGT, lärt av ett tidigare telemetry/cubic-fynd (PR #178):** en `withThrowingTaskGroup`-
/// baserad implementation (den ursprungliga versionen här) FUNGERAR INTE som
/// en riktig backstop mot en `operation` som blockerar synkront (t.ex. en
/// tråd i `DispatchSemaphore.wait()`, som `TailscaleStatus.fetchLocal`s
/// `ResultThread`) — strukturerad concurrency GARANTERAR att gruppen väntar
/// in ALLA barn-tasks innan den returnerar, även avbrutna sådana som aldrig
/// kollar `Task.isCancelled` (cancellation är kooperativt, inte
/// tvingande). Ett `cancelAll()` i en `defer` gör alltså ingenting mot en
/// tråd som redan sitter fast i ett OS-blockerande anrop — timeouten hade
/// bara framstått som om den fungerade för RIKTIGA async/await-operationer
/// som faktiskt ger upp sin tråd vid varje suspension point.
///
/// Den här versionen undviker problemet genom att INTE strukturellt vänta in
/// operation-tasken: den körs som en fristående (icke-grupperad) `Task`, och
/// vinnaren (operation ELLER timeout) löser en delad `continuation` en enda
/// gång via `TimeoutRace`. Om timeouten vinner returnerar/kastar funktionen
/// omedelbart — den hängande operation-tasken lämnas köra i bakgrunden
/// (accepterad tråd-läcka i just testsammanhang, hellre än att CI hänger
/// tills jobbets EGEN, mycket längre, timeout till slut ger upp).
func withTimeout<T: Sendable>(
    seconds: Double, _ operation: @escaping @Sendable () async throws -> T
) async throws -> T {
    let race = TimeoutRace<T>()
    return try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<T, Error>) in
        race.start(continuation)
        Task {
            do {
                let value = try await operation()
                race.resume(with: .success(value))
            } catch {
                race.resume(with: .failure(error))
            }
        }
        Task {
            try? await Task.sleep(for: .seconds(seconds))
            race.resume(with: .failure(TestTimeoutError(seconds: seconds)))
        }
    }
}

final class TerminalTeardownRaceTests: XCTestCase {
    func testCloseDuringActiveShellTerminatesOutputStreamPromptly() async throws {
        let server = try LoopbackServer.start(password: "hunter2")
        defer { server.shutdown() }

        let session = SSHSession(
            target: SSHTarget(host: "127.0.0.1", port: server.port, username: "tester"),
            auth: .password("hunter2"), knownHosts: KnownHosts(path: nil))
        try await session.connect()
        let shell = try await session.openShell(cols: 80, rows: 24)

        // Motsvarar dismiss MEDAN sessionen är aktiv (ingen väntan på
        // specifikt innehåll) - precis det scenario stop() i
        // SSHTerminalController hanterar.
        shell.close()
        await session.close()

        // close() signalerar `fatal`, vilket avslutar strömmen med ett kastat
        // SSHError (se openShell()) snarare än en tyst completion - exakt det
        // TerminalView.swifts start()-loop redan förväntar sig och fångar via
        // sin egen `catch { guard !isStopped else { return } ... }`. Om
        // teardown inte faktiskt stänger kanalen hänger den här loopen för
        // evigt istället och testet timear ut i CI - en giltig (om än mindre
        // precis) regressionsdetektor.
        var chunkCount = 0
        do {
            for try await _ in shell.output {
                chunkCount += 1
            }
            XCTFail("strömmen skulle avslutas med ett kastat fel efter close(), inte tyst")
        } catch let error as SSHError {
            guard case .channelFailed = error else {
                XCTFail("förväntade SSHError.channelFailed, fick \(error)")
                return
            }
            // Förväntat: close() -> signalFatal -> strömmen kastar.
        }
        XCTAssertEqual(chunkCount, 0, "ingen mer data skulle levereras efter close()")
    }

    func testCloseImmediatelyAfterConnectLeavesNoOrphanedSession() async throws {
        // Motsvarar det ursprungliga CodeRabbit-fyndet på #155: dismiss
        // som landar precis EFTER connect() men FÖRE/UNDER openShell().
        let server = try LoopbackServer.start(password: "hunter2")
        defer { server.shutdown() }

        let session = SSHSession(
            target: SSHTarget(host: "127.0.0.1", port: server.port, username: "tester"),
            auth: .password("hunter2"), knownHosts: KnownHosts(path: nil))
        try await session.connect()
        await session.close()

        // Sessionen är stängd - en efterföljande openShell() ska faila
        // rent (kanalen är borta) istället för att hänga eller lyckas mot
        // en föräldralös anslutning servern redan tappat räkningen på.
        do {
            _ = try await session.openShell(cols: 80, rows: 24)
            XCTFail("openShell() efter close() borde faila, inte lyckas mot en död session")
        } catch let error as SSHError {
            guard case .channelFailed = error else {
                XCTFail("förväntade SSHError.channelFailed, fick \(error)")
                return
            }
            // Förväntat: kanalen är stängd.
        }
    }

    // Till skillnad från testerna ovan (som anropar close() STRIKT efter att
    // openShell() redan returnerat) startar det här testet openShell() och
    // close() som två obundna Tasks UTAN inbördes ordning - matchar exakt
    // hur App/TerminalView.swift faktiskt använder SSHCore: start() anropar
    // connect() följt av openShell() i EN Task, medan stop() (utlöst av en
    // användares dismiss-knapptryck NÄR SOM HELST) anropar session.close()
    // från en HELT OBEROENDE Task - se rad 46-77 där ingenting synkroniserar
    // de två.
    //
    // 200 iterationer bevisade (empiriskt, mot 7bcc68b) att den ursprungliga
    // TOCTOU-luckan (close() startar EFTER isClosingOrClosed-kollen men
    // INNAN pipeline-uppslagningen svarar) är stängd: 0 kraschar av 200,
    // mot deterministisk hängning inom enstaka iterationer på den gamla
    // koden.
    //
    // En ÄNNU smalare lucka (close() exakt mellan att pipeline-uppslagningen
    // lyckas och att child-kanalen hinner skapas) reproducerades separat vid
    // ~3000 tighta iterationer utan paus - men den kräver att en användares
    // knapptryck landar i ett sub-millisekunds internt fönster MELLAN två
    // NIO-callbacks, något en riktig UI-interaktion aldrig producerar (jfr.
    // TerminalView.swift: stop() reagerar på en hel knapptryck-till-Task-
    // schemaläggning, storleksordningar långsammare). Bedömd som teoretisk,
    // inte praktiskt nåbar - se PR #169-diskussionen för resonemanget i sin
    // helhet. Om framtida telemetri (extern telemetry) visar motsatsen, återuppta med
    // en riktig lösning (t.ex. att synkronisera close() mot ett aktivt
    // event loop-jobb istället för att bara vänta in det).
    func testConcurrentOpenShellAndCloseNeverCrashes() async throws {
        // Denna specifika test kraschade RIKTIGT på macOS-CI 2026-07-18 (NIOs
        // "leaking promise"-läckagedetektor, se PR #172/#173-CI-historiken) —
        // exakt den "ännu smalare lucka" kommentaren ovan bedömde teoretisk.
        // En framtida regression i samma familj (t.ex. en långsammare CI-
        // runner som träffar racet oftare) ska INTE tillåtas blockera hela
        // CI-kön i en timme genom att bara hänga — 60s är gott om marginal
        // för 200 riktiga loopback-handskakningar (normalt <1s totalt), men
        // stoppar en genuin hängning snabbt och synligt istället.
        try await withTimeout(seconds: 60) {
            try await Self.runConcurrentOpenShellAndCloseIterations()
        }
    }

    private static func runConcurrentOpenShellAndCloseIterations() async throws {
        for _ in 0..<200 {
            let server = try LoopbackServer.start(password: "hunter2")
            defer { server.shutdown() }

            let session = SSHSession(
                target: SSHTarget(host: "127.0.0.1", port: server.port, username: "tester"),
                auth: .password("hunter2"), knownHosts: KnownHosts(path: nil))
            try await session.connect()

            await withTaskGroup(of: Void.self) { group in
                group.addTask {
                    do {
                        _ = try await session.openShell(cols: 80, rows: 24)
                    } catch let error as SSHError {
                        guard case .channelFailed = error else {
                            XCTFail("förväntade SSHError.channelFailed, fick \(error)")
                            return
                        }
                        // Förväntat: openShell() racade mot close() och förlorade rent.
                    } catch {
                        XCTFail("oväntad feltyp: \(error)")
                    }
                }
                group.addTask {
                    await session.close()
                }
                await group.waitForAll()
            }
        }
    }

    /// Samma race som ovan, men mot `execute()` istället för `openShell()` —
    /// `execute()` hade INTE ens det gamla (ofullständiga) #169-skyddet
    /// (ingen `resolveOnce`/`isClosingOrClosed`-koll), samma sårbarhetsklass
    /// men mer exponerad. Verifierar att den delade beginChildOp()/
    /// endChildOp()/waitForChildOpsToDrain()-mekanismen i SSHSession.swift
    /// även skyddar denna kodväg, inte bara openShell().
    func testConcurrentExecuteAndCloseNeverCrashes() async throws {
        try await withTimeout(seconds: 60) {
            try await Self.runConcurrentExecuteAndCloseIterations()
        }
    }

    private static func runConcurrentExecuteAndCloseIterations() async throws {
        for _ in 0..<200 {
            let server = try LoopbackServer.start(password: "hunter2")
            defer { server.shutdown() }

            let session = SSHSession(
                target: SSHTarget(host: "127.0.0.1", port: server.port, username: "tester"),
                auth: .password("hunter2"), knownHosts: KnownHosts(path: nil))
            try await session.connect()

            await withTaskGroup(of: Void.self) { group in
                group.addTask {
                    do {
                        for try await _ in session.execute("echo hej") {}
                    } catch let error as SSHError {
                        guard case .channelFailed = error else {
                            XCTFail("förväntade SSHError.channelFailed, fick \(error)")
                            return
                        }
                        // Förväntat: execute() racade mot close() och förlorade rent.
                    } catch {
                        XCTFail("oväntad feltyp: \(error)")
                    }
                }
                group.addTask {
                    await session.close()
                }
                await group.waitForAll()
            }
        }
    }

    /// Samma race, men mot `SFTPClient.open(on:)` — den gick tidigare via en
    /// fristående funktion UTANFÖR `SSHSession` och fick aldrig samma
    /// beginChildOp()/endChildOp()-registrering som execute()/openShell()
    /// (cubic P1 på PR #186: `close()` kunde riva event loop-gruppen medan
    /// `SFTPClient.open`s egen `createChannel`-promise fortfarande var
    /// olöst). Verifierar att öppningen nu räknas in i sessionens
    /// dräneringsräknare precis som de andra två kanaltyperna.
    func testConcurrentSFTPOpenAndCloseNeverCrashes() async throws {
        try await withTimeout(seconds: 60) {
            try await Self.runConcurrentSFTPOpenAndCloseIterations()
        }
    }

    private static func runConcurrentSFTPOpenAndCloseIterations() async throws {
        for _ in 0..<200 {
            let server = try LoopbackServer.start(password: "hunter2")
            defer { server.shutdown() }

            let session = SSHSession(
                target: SSHTarget(host: "127.0.0.1", port: server.port, username: "tester"),
                auth: .password("hunter2"), knownHosts: KnownHosts(path: nil))
            try await session.connect()

            await withTaskGroup(of: Void.self) { group in
                group.addTask {
                    do {
                        let client = try await SFTPClient.open(on: session)
                        await client.close()
                    } catch let error as SSHError {
                        guard case .channelFailed = error else {
                            XCTFail("förväntade SSHError.channelFailed, fick \(error)")
                            return
                        }
                        // Förväntat: open() racade mot close() och förlorade rent.
                    } catch {
                        XCTFail("oväntad feltyp: \(error)")
                    }
                }
                group.addTask {
                    await session.close()
                }
                await group.waitForAll()
            }
        }
    }
}
