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

    init(channel: Channel, output: AsyncThrowingStream<SSHChunk, Error>, cols: Int, rows: Int) {
        self.channel = channel
        self.output = output
        self.lastSize = NIOLockedValueBox((cols: cols, rows: rows))
        self.keepAliveTask = NIOLockedValueBox(nil)
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
    /// Täcker bara "håll NAT-mappningen varm"-delen av resiliens — upptäckt
    /// av en redan DÖD anslutning (nätverksbyte, viloläge) och återanslutning
    /// är separata, ännu inte implementerade delar av samma roadmap-punkt.
    public func startKeepAlive(interval: Duration = .seconds(30)) {
        let task = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: interval)
                guard !Task.isCancelled, let self else { return }
                let size = self.lastSize.withLockedValue { $0 }
                self.sendWindowChange(cols: size.cols, rows: size.rows)
            }
        }
        keepAliveTask.withLockedValue { old in
            old?.cancel()
            old = task
        }
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

    init(term: String, cols: Int, rows: Int,
         continuation: AsyncThrowingStream<SSHChunk, Error>.Continuation) {
        self.term = term
        self.cols = cols
        self.rows = rows
        self.continuation = continuation
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
