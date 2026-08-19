import Crypto
import Foundation
import NIOCore
import NIOPosix
import NIOSSH
import NIOConcurrencyHelpers
@testable import SSHCore

// En minimal SSH-server i processen, enbart för test. Accepterar ett lösenord
// och svarar på exec-förfrågningar med "ran: <kommando>\n" + exit 0.

final class ServerAuth: NIOSSHServerUserAuthenticationDelegate {
    let password: String
    init(password: String) { self.password = password }

    var supportedAuthenticationMethods: NIOSSHAvailableUserAuthenticationMethods {
        [.password, .publicKey]
    }

    func requestReceived(
        request: NIOSSHUserAuthenticationRequest,
        responsePromise: EventLoopPromise<NIOSSHUserAuthenticationOutcome>
    ) {
        switch request.request {
        case .password(let p):
            responsePromise.succeed(p.password == password ? .success : .failure)
        case .publicKey:
            // NIOSSH har redan verifierat signaturen; testservern godtar nyckeln.
            responsePromise.succeed(.success)
        default:
            responsePromise.succeed(.failure)
        }
    }
}

final class ServerExecHandler: ChannelDuplexHandler, RemovableChannelHandler {
    typealias InboundIn = SSHChannelData
    typealias InboundOut = SSHChannelData
    typealias OutboundIn = SSHChannelData
    typealias OutboundOut = SSHChannelData

    private var shellMode = false
    private let sftpRoot: String
    private let realExec: Bool
    /// Räknar mottagna fönsterändringsförfrågningar — bevisar att
    /// `SSHShell.startKeepAlive` faktiskt skickar något över tråden, inte
    /// bara att den anropar en lokal metod. `nil` (default) i alla test
    /// som inte bryr sig, för att inte röra befintliga testers beteende.
    private let observedWindowChanges: NIOLockedValueBox<[(cols: Int, rows: Int)]>?
    /// Sätts till `true` för att låta servern TYSTNA: den slutar svara på
    /// kanalförfrågningar men håller anslutningen öppen. Det är så en död
    /// motpart ser ut från klientens håll, och det enda sättet att testa
    /// död-detektering utan att faktiskt dra ur en nätverkskabel.
    private let silent: NIOLockedValueBox<Bool>?

    /// `realExec`: `false` (default) — den ursprungliga fejkade
    /// "ran: <kommando>\n"-ekot, vad de flesta exec-tester (`SSHCoreTests`,
    /// `KeyParserTests`, `KnownHostsTests`, `ProxyJumpTests` m.fl.) redan
    /// bygger på och inte ska rubbas. `true` — kör kommandot GENUINT via
    /// `Process` (`/bin/sh -c`) med `sftpRoot` som arbetskatalog, samma
    /// sandlåda `ServerSFTPHandler` redan använder — krävs för att bevisa
    /// t.ex. `tar`/`zip`-arkivoperationer på RIKTIGA filer, inte bara att
    /// rätt kommandosträng skickades.
    init(
        sftpRoot: String, realExec: Bool = false,
        observedWindowChanges: NIOLockedValueBox<[(cols: Int, rows: Int)]>? = nil,
        silent: NIOLockedValueBox<Bool>? = nil
    ) {
        self.sftpRoot = sftpRoot
        self.realExec = realExec
        self.observedWindowChanges = observedWindowChanges
        self.silent = silent
    }

    func handlerAdded(context: ChannelHandlerContext) {
        _ = context.channel.setOption(ChannelOptions.allowRemoteHalfClosure, value: true)
    }

    private func runReal(_ command: String, context: ChannelHandlerContext) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", command]
        process.currentDirectoryURL = URL(fileURLWithPath: sftpRoot)
        let outPipe = Pipe()
        process.standardOutput = outPipe
        process.standardError = outPipe
        do {
            try process.run()
        } catch {
            var buf = context.channel.allocator.buffer(capacity: 32)
            buf.writeString("kunde inte starta: \(error)\n")
            context.writeAndFlush(wrapOutboundOut(SSHChannelData(type: .channel, data: .byteBuffer(buf))), promise: nil)
            context.triggerUserOutboundEvent(SSHChannelRequestEvent.ExitStatus(exitStatus: 127), promise: nil)
            context.close(promise: nil)
            return
        }
        let outputData = outPipe.fileHandleForReading.readDataToEndOfFile()
        var status: Int32 = 0
        waitpid(process.processIdentifier, &status, 0)
        let exitCode = (status >> 8) & 0xFF
        var buf = context.channel.allocator.buffer(capacity: outputData.count)
        buf.writeBytes(outputData)
        context.writeAndFlush(wrapOutboundOut(SSHChannelData(type: .channel, data: .byteBuffer(buf))), promise: nil)
        context.triggerUserOutboundEvent(SSHChannelRequestEvent.ExitStatus(exitStatus: Int(exitCode)), promise: nil)
        context.close(promise: nil)
    }

    func userInboundEventTriggered(context: ChannelHandlerContext, event: Any) {
        if let exec = event as? SSHChannelRequestEvent.ExecRequest {
            if realExec {
                runReal(exec.command, context: context)
                return
            }
            var buf = context.channel.allocator.buffer(capacity: 32)
            buf.writeString("ran: \(exec.command)\n")
            let data = SSHChannelData(type: .channel, data: .byteBuffer(buf))
            context.writeAndFlush(wrapOutboundOut(data), promise: nil)
            context.triggerUserOutboundEvent(SSHChannelRequestEvent.ExitStatus(exitStatus: 0), promise: nil)
            context.close(promise: nil)
        } else if event is SSHChannelRequestEvent.ShellRequest {
            // Enkel "echo-shell": speglar all stdin tillbaka som stdout.
            shellMode = true
        } else if let subsystem = event as? SSHChannelRequestEvent.SubsystemRequest, subsystem.subsystem == "sftp" {
            // Byter ut sig själv mot en dedikerad SFTP-handler för resten av
            // kanalens livstid — samma mönster som protokolluppgraderingar
            // (t.ex. HTTP -> WebSocket) i övriga NIO-ekosystemet.
            _ = context.pipeline.removeHandler(self)
            _ = context.pipeline.addHandler(ServerSFTPHandler(rootPath: sftpRoot))
        } else if let windowChange = event as? SSHChannelRequestEvent.WindowChangeRequest {
            observedWindowChanges?.withLockedValue {
                $0.append((cols: windowChange.terminalCharacterWidth, rows: windowChange.terminalRowHeight))
            }
        } else if let env = event as? SSHChannelRequestEvent.EnvironmentRequest {
            // swift-nio-ssh svarar INTE automatiskt på förfrågningar den
            // förstår — den fyrar bara händelsen in i pipelinen och lämnar
            // svaret till applikationen (`SSHChildChannel.handleInbound
            // ChannelRequest`: bara OKÄNDA förfrågningar får ett automatiskt
            // failure). En riktig sshd svarar; utan raden nedan skulle den
            // här testservern vara den enda motpart i världen som inte gör
            // det, och `SSHShell`s livlighetssond skulle se varje session
            // som död.
            if env.wantReply, silent?.withLockedValue({ $0 }) != true {
                context.triggerUserOutboundEvent(ChannelSuccessEvent(), promise: nil)
            }
        }
        // PseudoTerminalRequest ignoreras (accepteras implicit).
    }

    func channelRead(context: ChannelHandlerContext, data: NIOAny) {
        guard shellMode else { return }
        let channelData = unwrapInboundIn(data)
        guard case .byteBuffer = channelData.data else { return }
        context.writeAndFlush(wrapOutboundOut(channelData), promise: nil)
    }
}

/// Minimal SFTP-server (version 3) för test — backad av ett riktigt
/// temp-directory (verklig filsystemsemantik: `FileManager`/`FileHandle`),
/// inte bara ett protokolleko. Räcker för att verifiera `SFTPClient`s
/// kanaltrådning/ramning/id-matchning på riktigt, utan att bero på ett
/// riktigt `sftp-server`-underprocess (fragilt att brygga ihop med
/// NIOPipeBootstrap + Foundation.Process — ett eget, separat steg om det
/// någonsin behövs för djupare protokollkompatibilitetsverifiering).
final class ServerSFTPHandler: ChannelDuplexHandler {
    typealias InboundIn = SSHChannelData
    typealias InboundOut = SSHChannelData
    typealias OutboundIn = SSHChannelData
    typealias OutboundOut = SSHChannelData

    private enum Handle {
        case directory(remaining: [String])
        case file(FileHandle)
    }

    private let rootPath: String
    private var inbound = ByteBuffer()
    private var handles: [UInt64: Handle] = [:]
    private var nextHandleID: UInt64 = 0

    init(rootPath: String) {
        self.rootPath = rootPath
    }

    /// Håller sökvägar innanför `rootPath` — utan detta kunde en `../`
    /// eller absolut sökväg fly sandlådan och mutera godtyckliga filer
    /// testprocessen har åtkomst till (CodeRabbit-fynd, PR #38). En
    /// icke-existerande sökväg (`__blocked__`) gör att alla efterföljande
    /// filsystemsanrop svarar "hittades inte" på ett säkert sätt, utan att
    /// göra den här (icke-kastande) funktionen throwing.
    private func diskPath(for sftpPath: String) -> String {
        let trimmed = sftpPath.hasPrefix("/") ? String(sftpPath.dropFirst()) : sftpPath
        let candidate = trimmed.isEmpty || trimmed == "." ? rootPath : rootPath + "/" + trimmed
        let standardized = (candidate as NSString).standardizingPath
        guard standardized == rootPath || standardized.hasPrefix(rootPath + "/") else {
            return rootPath + "/__blocked_path_traversal__"
        }
        return standardized
    }

    private func allocateHandle(_ handle: Handle) -> [UInt8] {
        let id = nextHandleID
        nextHandleID += 1
        handles[id] = handle
        return withUnsafeBytes(of: id.bigEndian, Array.init)
    }

    /// Bygger UInt64 byte-för-byte istället för `withUnsafeBytes { $0.load(as:) }`
    /// — den senare kan trappa på en `[UInt8]` vars underliggande lagring
    /// inte råkar vara 8-byte-alignad (CodeRabbit-fynd, PR #38).
    private func handleID(from bytes: [UInt8]) -> UInt64? {
        guard bytes.count == 8 else { return nil }
        var value: UInt64 = 0
        for byte in bytes { value = (value << 8) | UInt64(byte) }
        return value
    }

    func channelRead(context: ChannelHandlerContext, data: NIOAny) {
        let channelData = unwrapInboundIn(data)
        guard case .byteBuffer(var incoming) = channelData.data else { return }
        inbound.writeBuffer(&incoming)
        while true {
            guard let length: UInt32 = inbound.getInteger(at: inbound.readerIndex) else { break }
            guard inbound.readableBytes >= 4 + Int(length) else { break }
            _ = inbound.readInteger(as: UInt32.self)
            guard var payload = inbound.readSlice(length: Int(length)) else { break }
            process(&payload, context: context)
        }
    }

    private func send(_ packet: ByteBuffer, context: ChannelHandlerContext) {
        let data = SSHChannelData(type: .channel, data: .byteBuffer(packet))
        context.writeAndFlush(wrapOutboundOut(data), promise: nil)
    }

    private func sendStatus(id: UInt32, code: SFTPStatusCode, message: String = "", context: ChannelHandlerContext) {
        let packet = makeSFTPPacket(type: .status) { buf in
            buf.writeInteger(id)
            buf.writeInteger(code.rawValue)
            buf.writeSFTPString(message)
            buf.writeSFTPString("")
        }
        send(packet, context: context)
    }

    private func sendHandle(id: UInt32, handle: [UInt8], context: ChannelHandlerContext) {
        let packet = makeSFTPPacket(type: .handle) { buf in
            buf.writeInteger(id)
            buf.writeSFTPBytes(handle)
        }
        send(packet, context: context)
    }

    private func attributes(atDiskPath path: String) -> SFTPFileAttributes {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: path) else { return SFTPFileAttributes() }
        let size = (attrs[.size] as? NSNumber)?.uint64Value
        let isDir = (attrs[.type] as? FileAttributeType) == .typeDirectory
        let posixBits = (attrs[.posixPermissions] as? NSNumber)?.uint32Value ?? (isDir ? 0o755 : 0o644)
        // Riktiga SFTP-servrar lägger filtypen (S_IFDIR/S_IFREG) i samma
        // permissions-fält, inte bara de nio låga behörighetsbitarna —
        // annars kan en klient inte skilja mapp från fil utan ett extra
        // STAT-anrop per post.
        let typeBits: UInt32 = isDir ? 0o040000 : 0o100000
        return SFTPFileAttributes(size: size, permissions: posixBits | typeBits)
    }

    private func process(_ payload: inout ByteBuffer, context: ChannelHandlerContext) {
        guard let rawType: UInt8 = payload.readInteger(), let type = SFTPMessageType(rawValue: rawType) else { return }
        let fm = FileManager.default

        switch type {
        case .initMsg:
            _ = payload.readInteger(as: UInt32.self)
            send(makeSFTPPacket(type: .version) { $0.writeInteger(UInt32(3)) }, context: context)

        case .realpath:
            guard let id: UInt32 = payload.readInteger(), let path = try? payload.readSFTPString() else { return }
            let resolved = path.hasPrefix("/") ? path : "/" + path
            let packet = makeSFTPPacket(type: .name) { buf in
                buf.writeInteger(id)
                buf.writeInteger(UInt32(1))
                buf.writeSFTPString(resolved)
                buf.writeSFTPString(resolved)
                SFTPFileAttributes().encode(into: &buf)
            }
            send(packet, context: context)

        case .opendir:
            guard let id: UInt32 = payload.readInteger(), let path = try? payload.readSFTPString() else { return }
            let disk = diskPath(for: path)
            guard let entries = try? fm.contentsOfDirectory(atPath: disk) else {
                sendStatus(id: id, code: .noSuchFile, message: "no such directory", context: context)
                return
            }
            let handle = allocateHandle(.directory(remaining: entries))
            sendHandle(id: id, handle: handle, context: context)

        case .readdir:
            // id läses ut separat FÖRST så en ogiltig handle-referens ändå
            // kan besvaras med rätt id — annars väntar klienten på den
            // pending-continuationen för evigt (CodeRabbit-fynd, PR #38).
            guard let id: UInt32 = payload.readInteger() else { return }
            guard let handleBytes = try? payload.readSFTPBytes(),
                  let hid = handleID(from: handleBytes), case .directory(let remaining) = handles[hid]
            else {
                sendStatus(id: id, code: .failure, context: context)
                return
            }
            guard !remaining.isEmpty else {
                sendStatus(id: id, code: .eof, context: context)
                return
            }
            let batch = remaining
            handles[hid] = .directory(remaining: [])
            let packet = makeSFTPPacket(type: .name) { buf in
                buf.writeInteger(id)
                buf.writeInteger(UInt32(batch.count))
                for name in batch {
                    buf.writeSFTPString(name)
                    buf.writeSFTPString(name)
                    attributes(atDiskPath: rootPath + "/" + name).encode(into: &buf)
                }
            }
            send(packet, context: context)

        case .close:
            guard let id: UInt32 = payload.readInteger() else { return }
            guard let handleBytes = try? payload.readSFTPBytes(), let hid = handleID(from: handleBytes)
            else {
                sendStatus(id: id, code: .failure, context: context)
                return
            }
            if case .file(let fh) = handles[hid] { try? fh.close() }
            handles.removeValue(forKey: hid)
            sendStatus(id: id, code: .ok, context: context)

        case .open:
            guard let id: UInt32 = payload.readInteger(), let path = try? payload.readSFTPString(),
                  let flags: UInt32 = payload.readInteger()
            else { return }
            _ = try? SFTPFileAttributes.decode(from: &payload)
            let disk = diskPath(for: path)
            let openFlags = SFTPOpenFlags(rawValue: flags)
            if openFlags.contains(.create) && !fm.fileExists(atPath: disk) {
                _ = fm.createFile(atPath: disk, contents: nil)
            }
            guard fm.fileExists(atPath: disk) else {
                sendStatus(id: id, code: .noSuchFile, context: context)
                return
            }
            if openFlags.contains(.truncate) {
                try? Data().write(to: URL(fileURLWithPath: disk))
            }
            let fh = openFlags.contains(.write)
                ? FileHandle(forUpdatingAtPath: disk) : FileHandle(forReadingAtPath: disk)
            guard let fh else {
                sendStatus(id: id, code: .permissionDenied, context: context)
                return
            }
            let handle = allocateHandle(.file(fh))
            sendHandle(id: id, handle: handle, context: context)

        case .read:
            guard let id: UInt32 = payload.readInteger() else { return }
            guard let handleBytes = try? payload.readSFTPBytes(),
                  let hid = handleID(from: handleBytes), let offset: UInt64 = payload.readInteger(),
                  let length: UInt32 = payload.readInteger(), case .file(let fh) = handles[hid]
            else {
                sendStatus(id: id, code: .failure, context: context)
                return
            }
            try? fh.seek(toOffset: offset)
            let data = (try? fh.read(upToCount: Int(length))) ?? nil
            guard let data, !data.isEmpty else {
                sendStatus(id: id, code: .eof, context: context)
                return
            }
            let packet = makeSFTPPacket(type: .data) { buf in
                buf.writeInteger(id)
                buf.writeSFTPBytes(Array(data))
            }
            send(packet, context: context)

        case .write:
            guard let id: UInt32 = payload.readInteger() else { return }
            guard let handleBytes = try? payload.readSFTPBytes(),
                  let hid = handleID(from: handleBytes), let offset: UInt64 = payload.readInteger(),
                  let bytes = try? payload.readSFTPBytes(), case .file(let fh) = handles[hid]
            else {
                sendStatus(id: id, code: .failure, context: context)
                return
            }
            try? fh.seek(toOffset: offset)
            try? fh.write(contentsOf: Data(bytes))
            sendStatus(id: id, code: .ok, context: context)

        case .stat, .lstat:
            guard let id: UInt32 = payload.readInteger(), let path = try? payload.readSFTPString() else { return }
            let disk = diskPath(for: path)
            guard fm.fileExists(atPath: disk) else {
                sendStatus(id: id, code: .noSuchFile, context: context)
                return
            }
            let packet = makeSFTPPacket(type: .attrs) { buf in
                buf.writeInteger(id)
                attributes(atDiskPath: disk).encode(into: &buf)
            }
            send(packet, context: context)

        case .mkdir:
            guard let id: UInt32 = payload.readInteger(), let path = try? payload.readSFTPString() else { return }
            do {
                try fm.createDirectory(atPath: diskPath(for: path), withIntermediateDirectories: false)
                sendStatus(id: id, code: .ok, context: context)
            } catch {
                sendStatus(id: id, code: .failure, message: "\(error)", context: context)
            }

        case .rmdir, .remove:
            guard let id: UInt32 = payload.readInteger(), let path = try? payload.readSFTPString() else { return }
            do {
                try fm.removeItem(atPath: diskPath(for: path))
                sendStatus(id: id, code: .ok, context: context)
            } catch {
                sendStatus(id: id, code: .noSuchFile, message: "\(error)", context: context)
            }

        case .rename:
            guard let id: UInt32 = payload.readInteger(), let oldPath = try? payload.readSFTPString(),
                  let newPath = try? payload.readSFTPString()
            else { return }
            do {
                try fm.moveItem(atPath: diskPath(for: oldPath), toPath: diskPath(for: newPath))
                sendStatus(id: id, code: .ok, context: context)
            } catch {
                sendStatus(id: id, code: .failure, message: "\(error)", context: context)
            }

        case .setstat:
            guard let id: UInt32 = payload.readInteger(), let path = try? payload.readSFTPString() else { return }
            do {
                let attrs = try SFTPFileAttributes.decode(from: &payload)
                var fileAttrs: [FileAttributeKey: Any] = [:]
                if let mode = attrs.permissions { fileAttrs[.posixPermissions] = mode }
                // `chown` till en ANNAN uid/gid än processens egen kräver root
                // (normalt POSIX-beteende) — testservern körs oprivilegierad,
                // så bara ett "byte" till samma ägare den redan har går
                // igenom här. Räcker för att bevisa hela protokollvägen
                // (klient -> SETSTAT med uid/gid -> server -> riktig fil).
                if let uid = attrs.uid { fileAttrs[.ownerAccountID] = uid }
                if let gid = attrs.gid { fileAttrs[.groupOwnerAccountID] = gid }
                if !fileAttrs.isEmpty {
                    try fm.setAttributes(fileAttrs, ofItemAtPath: diskPath(for: path))
                }
                sendStatus(id: id, code: .ok, context: context)
            } catch {
                sendStatus(id: id, code: .failure, message: "\(error)", context: context)
            }

        case .fsetstat, .fstat:
            guard let id: UInt32 = payload.readInteger() else { return }
            sendStatus(id: id, code: .opUnsupported, context: context)

        case .version, .status, .handle, .data, .name, .attrs:
            break  // svarstyper — kommer aldrig från en klient
        }
    }

    func channelInactive(context: ChannelHandlerContext) {
        for case .file(let fh) in handles.values { try? fh.close() }
        handles.removeAll()
        context.fireChannelInactive()
    }
}

/// Server-sidans direct-tcpip-handler för test: ekar bara tillbaka precis vad
/// den tar emot, oavsett vilken targetHost/targetPort klienten bad om. Räcker
/// för att bevisa hela portvidarebefordrings-vägen (lokal TCP -> SSH-kanal ->
/// tillbaka) utan att behöva ett riktigt nätverksmål.
final class ServerDirectTCPIPEchoHandler: ChannelDuplexHandler {
    typealias InboundIn = SSHChannelData
    typealias InboundOut = SSHChannelData
    typealias OutboundIn = SSHChannelData
    typealias OutboundOut = SSHChannelData

    func channelRead(context: ChannelHandlerContext, data: NIOAny) {
        context.writeAndFlush(data, promise: nil)
    }
}

/// Server-sidans RIKTIGA direct-tcpip-vidarebefordrare (till skillnad från
/// `ServerDirectTCPIPEchoHandler` ovan) — öppnar en riktig utgående TCP-
/// anslutning till klientens begärda `targetHost:targetPort` och bryggar
/// den mot SSH-kanalen med `GlueHandler`, exakt samma mönster som Bastions
/// egen klientsida använder för `-L`/`-D`. Behövs för ProxyJump-testet:
/// en jump-server som bara ekar skulle inte bevisa att kedjningen faktiskt
/// når en SEPARAT, riktig målserver.
func makeRealDirectTCPIPForwarder(group: EventLoopGroup) -> (Channel, SSHChannelType) -> EventLoopFuture<Void> {
    { childChannel, channelType in
        guard case .directTCPIP(let info) = channelType else {
            return childChannel.eventLoop.makeFailedFuture(SSHError.channelFailed("fel kanaltyp"))
        }
        return ClientBootstrap(group: group)
            .connect(host: info.targetHost, port: info.targetPort)
            .flatMap { targetChannel in
                targetChannel.eventLoop.makeCompletedFuture {
                    let (ours, theirs) = GlueHandler.matchedPair()
                    try targetChannel.pipeline.syncOperations.addHandler(ours)
                    try childChannel.pipeline.syncOperations.addHandler(DirectTCPIPWrapperHandler())
                    try childChannel.pipeline.syncOperations.addHandler(theirs)
                }
            }
    }
}

/// Server-sidans stöd för fjärr-portvidarebefordran (`ssh -R`) i test: en
/// generisk vidarebefordrare (inte en fejk-eko som `ServerDirectTCPIPEchoHandler`
/// ovan) — binder en riktig lokal lyssnare och bryggar varje accepterad
/// anslutning mot en `forwarded-tcpip`-SSH-kanal tillbaka till klienten, precis
/// som en riktig SSH-server. Baserad på swift-nio-ssh:s eget exempel
/// (`Sources/NIOSSHServer/RemotePortForwarding.swift`, Apache 2.0) men
/// återanvänder Bastions egen `DirectTCPIPWrapperHandler` istället för deras
/// `DataToBufferCodec`.
final class ServerRemoteForwarder {
    private let inboundSSHHandler: NIOSSHHandler
    private var listenerChannel: Channel?

    init(inboundSSHHandler: NIOSSHHandler) {
        self.inboundSSHHandler = inboundSSHHandler
    }

    func beginListening(on host: String, port: Int, loop: EventLoop) -> EventLoopFuture<Int?> {
        let sshHandler = self.inboundSSHHandler
        return ServerBootstrap(group: loop)
            .serverChannelOption(ChannelOptions.socketOption(.so_reuseaddr), value: 1)
            .childChannelInitializer { childChannel in
                childChannel.eventLoop.makeCompletedFuture {
                    let (ours, theirs) = GlueHandler.matchedPair()
                    let promise = loop.makePromise(of: Channel.self)
                    sshHandler.createChannel(
                        promise,
                        channelType: .forwardedTCPIP(
                            .init(
                                listeningHost: host,
                                listeningPort: childChannel.localAddress!.port!,
                                originatorAddress: childChannel.remoteAddress!
                            )
                        )
                    ) { sshChildChannel, _ in
                        sshChildChannel.eventLoop.makeCompletedFuture {
                            try sshChildChannel.pipeline.syncOperations.addHandler(DirectTCPIPWrapperHandler())
                            try sshChildChannel.pipeline.syncOperations.addHandler(theirs)
                        }
                    }
                    try childChannel.pipeline.syncOperations.addHandler(ours)
                }
            }
            .bind(host: host, port: port)
            .map { channel in
                self.listenerChannel = channel
                return port == 0 ? channel.localAddress?.port : nil
            }
    }

    func stopListening() {
        // wait() kraschar med "BUG DETECTED" om den anropas på en event loop-
        // tråd, vilket den här metoden alltid gör (kallas synkront från
        // tcpForwardingRequest()s .cancel-gren). Best-effort, inget att
        // invänta i test.
        _ = listenerChannel?.close()
    }
}

final class ServerRemoteForwardingDelegate: GlobalRequestDelegate {
    // Testservern tolererar bara en bunden port åt gången — tillräckligt för
    // testerna, ingen riktig produktionsbegränsning.
    private var forwarder: ServerRemoteForwarder?

    func tcpForwardingRequest(
        _ request: GlobalRequest.TCPForwardingRequest,
        handler: NIOSSHHandler,
        promise: EventLoopPromise<GlobalRequest.TCPForwardingResponse>
    ) {
        switch request {
        case .listen(let host, let port):
            let forwarder = ServerRemoteForwarder(inboundSSHHandler: handler)
            self.forwarder = forwarder
            forwarder.beginListening(on: host, port: port, loop: promise.futureResult.eventLoop)
                .map { GlobalRequest.TCPForwardingResponse(boundPort: $0) }
                .cascade(to: promise)
        case .cancel:
            forwarder?.stopListening()
            forwarder = nil
            promise.succeed(GlobalRequest.TCPForwardingResponse(boundPort: nil))
        }
    }
}

struct LoopbackServer {
    let group: MultiThreadedEventLoopGroup
    let channel: Channel
    /// Eget temp-directory per server-instans — SFTP-testerna läser/skriver
    /// riktiga filer här, aldrig i den delade systemtemp-mappen direkt.
    let sftpRoot: String
    /// Varje `direct-tcpip`-begärans faktiska mål, i mottagningsordning —
    /// `ServerDirectTCPIPEchoHandler` ekar oavsett mål, så det här är enda
    /// sättet ett test kan bevisa att KLIENTEN (t.ex. `-D`/SOCKS) faktiskt
    /// begärde rätt targetHost/targetPort, oberoende av vad servern gör med det.
    let observedDirectTCPIPTargets: NIOLockedValueBox<[(host: String, port: Int)]>
    /// `nil` om `start(...)` inte bad om det (default) — se `ServerExecHandler`.
    let observedWindowChanges: NIOLockedValueBox<[(cols: Int, rows: Int)]>?
    /// `nil` om `start(silenceable:)` inte bad om det. Sätt till `true` för
    /// att låta servern sluta svara utan att stänga anslutningen.
    let silent: NIOLockedValueBox<Bool>?
    var port: Int { channel.localAddress?.port ?? 0 }

    /// `realDirectTCPIPForwarding`: `false` (default) ekar bara tillbaka
    /// (räcker för `-L`/`-D`-tester som bara bevisar rundresan genom
    /// tunneln). `true` öppnar en RIKTIG utgående anslutning till klientens
    /// begärda mål — krävs för att testa ProxyJump på riktigt (jump-servern
    /// måste faktiskt nå en separat målserver, inte bara eka).
    /// `trackWindowChanges`: `false` (default) — sätt till `true` för att
    /// låta `observedWindowChanges` faktiskt samla in fönsterändringar
    /// (t.ex. `SSHShell.startKeepAlive`-tester).
    static func start(
        password: String, realDirectTCPIPForwarding: Bool = false, realExec: Bool = false,
        trackWindowChanges: Bool = false, silenceable: Bool = false
    ) throws -> LoopbackServer {
        let group = MultiThreadedEventLoopGroup(numberOfThreads: 1)
        let hostKey = NIOSSHPrivateKey(ed25519Key: .init())
        let sftpRoot = NSTemporaryDirectory() + "bastion-sftp-test-\(UUID().uuidString)"
        try FileManager.default.createDirectory(atPath: sftpRoot, withIntermediateDirectories: true)
        let observedDirectTCPIPTargets = NIOLockedValueBox<[(host: String, port: Int)]>([])
        let observedWindowChanges: NIOLockedValueBox<[(cols: Int, rows: Int)]>? =
            trackWindowChanges ? NIOLockedValueBox([]) : nil
        let silent: NIOLockedValueBox<Bool>? = silenceable ? NIOLockedValueBox(false) : nil
        let realForwarder = makeRealDirectTCPIPForwarder(group: group)
        let bootstrap = ServerBootstrap(group: group)
            .childChannelInitializer { channel in
                channel.pipeline.addHandler(
                    NIOSSHHandler(
                        role: .server(.init(
                            hostKeys: [hostKey],
                            userAuthDelegate: ServerAuth(password: password),
                            globalRequestDelegate: ServerRemoteForwardingDelegate())),
                        allocator: channel.allocator,
                        inboundChildChannelInitializer: { child, channelType in
                            switch channelType {
                            case .directTCPIP(let info):
                                observedDirectTCPIPTargets.withLockedValue {
                                    $0.append((host: info.targetHost, port: info.targetPort))
                                }
                                if realDirectTCPIPForwarding {
                                    return realForwarder(child, channelType)
                                }
                                return child.pipeline.addHandler(ServerDirectTCPIPEchoHandler())
                            default:
                                return child.pipeline.addHandler(
                                    ServerExecHandler(
                                        sftpRoot: sftpRoot, realExec: realExec,
                                        observedWindowChanges: observedWindowChanges,
                                        silent: silent))
                            }
                        }))
            }
        let channel = try bootstrap.bind(host: "127.0.0.1", port: 0).wait()
        return LoopbackServer(
            group: group, channel: channel, sftpRoot: sftpRoot,
            observedDirectTCPIPTargets: observedDirectTCPIPTargets,
            observedWindowChanges: observedWindowChanges,
            silent: silent)
    }

    func shutdown() {
        try? channel.close().wait()
        try? group.syncShutdownGracefully()
        try? FileManager.default.removeItem(atPath: sftpRoot)
    }
}
