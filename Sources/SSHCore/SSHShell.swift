import NIOConcurrencyHelpers
import NIOCore
import NIOSSH

/// En interaktiv shell över SSH: PTY + shell-kanal. `output` strömmar allt
/// servern skriver; `send`/`resize` matar tangenttryck och fönsterändringar.
/// Detta är kanalen som driver en riktig terminalvy (SwiftTerm).
public final class SSHShell: @unchecked Sendable {
    private let channel: Channel
    public let output: AsyncThrowingStream<SSHChunk, Error>
    // Delas mellan anroparens tråd (resize/startKeepAlive/close, typiskt
    // UI-tråden) och keepAlive-Task:ens egen exekveringskontext — därav
    // NIOLockedValueBox i stället för vanliga `var`, precis som
    // PortForward.swift/SOCKSProxy.swift redan gör för motsvarande delat
    // tillstånd.
    private let lastSize: NIOLockedValueBox<(cols: Int, rows: Int)>
    private let keepAliveTask: NIOLockedValueBox<Task<Void, Never>?>
    /// Räknas upp av `ShellHandler` varje gång servern svarar på en
    /// kanalförfrågan. Delas mellan kanalens event loop (som skriver) och
    /// keep-alive-Task:en (som läser) — därav samma låsbox som resten.
    private let liveness: NIOLockedValueBox<Int>

    init(
        channel: Channel, output: AsyncThrowingStream<SSHChunk, Error>,
        cols: Int, rows: Int, liveness: NIOLockedValueBox<Int>
    ) {
        self.channel = channel
        self.output = output
        self.lastSize = NIOLockedValueBox((cols: cols, rows: rows))
        self.keepAliveTask = NIOLockedValueBox(nil)
        self.liveness = liveness
    }

    /// Skicka rå indata (tangenttryck) till fjärr-shellen.
    public func send(_ bytes: [UInt8]) {
        var buf = channel.allocator.buffer(capacity: bytes.count)
        buf.writeBytes(bytes)
        channel.writeAndFlush(buf, promise: nil)
    }

    public func send(_ text: String) {
        send(Array(text.utf8))
    }

    /// Meddela servern att terminalen ändrat storlek (SIGWINCH på fjärrsidan).
    public func resize(cols: Int, rows: Int) {
        lastSize.withLockedValue { $0 = (cols: cols, rows: rows) }
        sendWindowChange(cols: cols, rows: rows)
    }

    private func sendWindowChange(cols: Int, rows: Int) {
        let ev = SSHChannelRequestEvent.WindowChangeRequest(
            terminalCharacterWidth: cols, terminalRowHeight: rows,
            terminalPixelWidth: 0, terminalPixelHeight: 0)
        channel.triggerUserOutboundEvent(ev, promise: nil)
    }

    /// Startar ett periodiskt no-op-fönsterändringsmeddelande (samma storlek
    /// som senast kända — ingen faktisk ändring) för att hålla anslutningen
    /// vid liv genom NAT/brandväggars idle-timeout. swift-nio-ssh exponerar
    /// ingen generisk global request (`sendTCPForwardingRequest` är den enda
    /// publika), så ett riktigt `keepalive@openssh.com`-liknande meddelande
    /// går inte att skicka (se ROADMAP.md "Anslutnings-resiliens") — en
    /// oförändrad fönsterändring är däremot redan en publik, väl beprövad
    /// kanalförfrågan. Ofarlig: Linux TTY-drivrutinen (`TIOCSWINSZ`) skickar
    /// bara SIGWINCH till fjärrprocessen vid en FAKTISK ändring (`tty_ioctl.c`
    /// jämför mot föregående storlek) — samma storlek som redan är satt ger
    /// alltså ingen synlig effekt i den körande shell-sessionen.
    /// Fönsterändringen ensam kan bara hålla NAT varm, aldrig UPPTÄCKA att
    /// anslutningen dött: `WindowChangeRequest.wantReply` är hårdkodad
    /// `false` i swift-nio-ssh, så det finns inget uteblivet svar att
    /// sakna. Därför skickas även en svar-bärande sond, se
    /// `sendLivenessProbe()`. Uteblir svaret `maxMissed` gånger i rad är
    /// anslutningen död: `onConnectionLost` anropas och kanalen stängs, i
    /// stället för att sessionen sitter och väntar för alltid.
    ///
    /// `onConnectionLost` körs på keep-alive-Task:ens kontext, inte på
    /// någon UI-tråd — anroparen får hoppa till rätt kontext själv.
    public func startKeepAlive(
        interval: Duration = .seconds(30),
        maxMissed: Int = 3,
        onConnectionLost: (@Sendable () -> Void)? = nil
    ) {
        let task = Task { [weak self] in
            var missed = 0
            while !Task.isCancelled {
                // `self?` bara i korta steg: Task:en ska inte hålla liv i
                // shellen mellan varven bara för att den råkar vänta.
                guard let before = self?.probeAndReadLiveness() else { return }
                try? await Task.sleep(for: interval)
                guard !Task.isCancelled, let now = self?.currentLiveness() else { return }

                if now == before {
                    missed += 1
                    if missed >= maxMissed {
                        onConnectionLost?()
                        self?.close()
                        return
                    }
                } else {
                    missed = 0
                }
            }
        }
        keepAliveTask.withLockedValue { old in
            old?.cancel()
            old = task
        }
    }

    /// Skickar båda meddelandena och returnerar räknarställningen FÖRE dem,
    /// så anroparen kan se om något svar kom under väntan. `nil` när kanalen
    /// inte längre är aktiv — då finns inget att mäta, och en förfrågan på
    /// en stängd kanal är dessutom ett protokollfel som river kanalen.
    private func probeAndReadLiveness() -> Int? {
        guard channel.isActive else { return nil }
        let before = liveness.withLockedValue { $0 }
        let size = lastSize.withLockedValue { $0 }
        sendWindowChange(cols: size.cols, rows: size.rows)
        sendLivenessProbe()
        return before
    }

    private func currentLiveness() -> Int {
        liveness.withLockedValue { $0 }
    }

    /// Sonden som gör död-detektering möjlig: en kanalförfrågan servern
    /// MÅSTE besvara.
    ///
    /// `EnvironmentRequest` är vald för att den är ofarlig. En miljövariabel
    /// som sätts efter att shellen redan startat påverkar ingenting —
    /// miljön appliceras när processen skapas. VILKET svar som kommer
    /// spelar heller ingen roll: success och failure bevisar båda att någon
    /// i andra änden lever och svarar.
    ///
    /// Verifierat mot en RIKTIG OpenSSH-sshd, inte antaget ur RFC 4254:
    /// en `env`-begäran med `want_reply` skickad EFTER `shell` besvarades
    /// med `SSH_MSG_CHANNEL_SUCCESS`. (Att läsa OpenSSH:s `session.c` gav
    /// gissningen att den skulle svara FAILURE eftersom kanalen inte längre
    /// är `LARVAL` — fel gissning, men irrelevant: båda svaren duger.)
    private func sendLivenessProbe() {
        let probe = SSHChannelRequestEvent.EnvironmentRequest(
            wantReply: true, name: "BASTION_KEEPALIVE", value: "1")
        channel.triggerUserOutboundEvent(probe, promise: nil)
    }

    public func stopKeepAlive() {
        keepAliveTask.withLockedValue { task in
            task?.cancel()
            task = nil
        }
    }

    public func close() {
        stopKeepAlive()
        channel.close(promise: nil)
    }
}

/// Barnkanal-handler för en interaktiv shell. Begär PTY + shell vid uppkoppling,
/// strömmar utdata och slår om ByteBuffer <-> SSHChannelData för indata.
final class ShellHandler: ChannelDuplexHandler {
    typealias InboundIn = SSHChannelData
    typealias InboundOut = Never
    typealias OutboundIn = ByteBuffer
    typealias OutboundOut = SSHChannelData

    private let term: String
    private let cols: Int
    private let rows: Int
    private let continuation: AsyncThrowingStream<SSHChunk, Error>.Continuation
    private let liveness: NIOLockedValueBox<Int>

    init(term: String, cols: Int, rows: Int,
         continuation: AsyncThrowingStream<SSHChunk, Error>.Continuation,
         liveness: NIOLockedValueBox<Int>) {
        self.term = term
        self.cols = cols
        self.rows = rows
        self.continuation = continuation
        self.liveness = liveness
    }

    /// Enda mätpunkten för att fjärrsidan lever. Vilket av de två svaren
    /// det är spelar ingen roll — servern kan bara skicka något av dem om
    /// den faktiskt läser och behandlar vår trafik.
    func userInboundEventTriggered(context: ChannelHandlerContext, event: Any) {
        if event is ChannelSuccessEvent || event is ChannelFailureEvent {
            liveness.withLockedValue { $0 &+= 1 }
        }
        context.fireUserInboundEventTriggered(event)
    }

    func handlerAdded(context: ChannelHandlerContext) {
        _ = context.channel.setOption(ChannelOptions.allowRemoteHalfClosure, value: true)
    }

    func channelActive(context: ChannelHandlerContext) {
        // wantReply: false — vi blockerar inte på bekräftelse; PTY allokeras ändå.
        let pty = SSHChannelRequestEvent.PseudoTerminalRequest(
            wantReply: false, term: term,
            terminalCharacterWidth: cols, terminalRowHeight: rows,
            terminalPixelWidth: 0, terminalPixelHeight: 0,
            terminalModes: SSHTerminalModes([:]))
        context.triggerUserOutboundEvent(pty, promise: nil)
        let shell = SSHChannelRequestEvent.ShellRequest(wantReply: false)
        context.triggerUserOutboundEvent(shell, promise: nil)
        context.fireChannelActive()
    }

    func channelRead(context: ChannelHandlerContext, data: NIOAny) {
        let channelData = unwrapInboundIn(data)
        guard case .byteBuffer(let buf) = channelData.data else { return }
        let bytes = buf.getBytes(at: buf.readerIndex, length: buf.readableBytes) ?? []
        let stream: SSHChunk.Stream = channelData.type == .stdErr ? .stderr : .stdout
        continuation.yield(SSHChunk(stream: stream, bytes: bytes))
    }

    func channelInactive(context: ChannelHandlerContext) {
        continuation.finish()
        context.fireChannelInactive()
    }

    func errorCaught(context: ChannelHandlerContext, error: Error) {
        continuation.finish(throwing: error)
        context.close(promise: nil)
    }

    // indata: ByteBuffer -> SSHChannelData
    func write(context: ChannelHandlerContext, data: NIOAny, promise: EventLoopPromise<Void>?) {
        let buf = unwrapOutboundIn(data)
        let wrapped = SSHChannelData(type: .channel, data: .byteBuffer(buf))
        context.write(wrapOutboundOut(wrapped), promise: promise)
    }
}
