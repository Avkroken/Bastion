#if os(macOS) || os(Linux)
import XCTest
@testable import SSHCore

/// Plattformsoberoende felvägar — inget beroende på en riktig tty/PTY, körs
/// på både macOS och Linux.
final class SerialTests: XCTestCase {
    func testOpenNonexistentPathThrows() async throws {
        do {
            _ = try await SerialSession.connect(config: SerialConfig(path: "/dev/does-not-exist-bastion-test"))
            XCTFail("förväntade openFailed")
        } catch SerialError.openFailed {
            // förväntat
        }
    }
}
#endif

#if canImport(Darwin)
import Darwin

/// Verifierar `SerialSession` mot en RIKTIG pseudo-terminal (PTY) — inte en
/// mock. En PTY-slav beter sig som en äkta seriell tty ur `open()`/
/// `tcsetattr()`s perspektiv (samma drivrutinsfamilj), så testet bevisar att
/// `configureTermios`/`NIOPipeBootstrap`-vägen faktiskt fungerar mot en
/// riktig enhet, inte bara att koden kompilerar.
///
/// `posix_openpt`/`grantpt`/`unlockpt`/`ptsname` är bara pålitligt
/// deklarerade i den FULLA Darwin-modulen (saknas i vissa Linux Glibc-
/// overlays, t.ex. den dev-snapshot-toolchain som används lokalt på mp100,
/// se [[reference-mp100-swift-toolchain-linuxapp]]) — CI:s `swift test` för
/// SSHCore körs ändå bara på riktig macOS-hårdvara
/// (`.github/workflows/xcode.yml`, `runs-on: macos-26`), så `canImport(Darwin)`
/// här tappar ingen CI-täckning.
final class SerialPTYTests: XCTestCase {
    private func openPTYPair() throws -> (masterFD: Int32, slavePath: String) {
        let masterFD = posix_openpt(O_RDWR)
        guard masterFD >= 0 else { throw SerialError.openFailed("posix_openpt") }
        guard grantpt(masterFD) == 0, unlockpt(masterFD) == 0, let namePtr = ptsname(masterFD) else {
            Darwin.close(masterFD)
            throw SerialError.openFailed("grantpt/unlockpt/ptsname")
        }
        return (masterFD, String(cString: namePtr))
    }

    private func writeRaw(_ fd: Int32, _ bytes: [UInt8]) {
        var copy = bytes
        _ = copy.withUnsafeMutableBytes { buf in
            write(fd, buf.baseAddress, buf.count)
        }
    }

    func testSendReceiveRoundTripOverRealPTY() async throws {
        let (masterFD, slavePath) = try openPTYPair()
        defer { Darwin.close(masterFD) }

        let session = try await SerialSession.connect(config: SerialConfig(path: slavePath, baudRate: 9600))

        // Master -> session.output ("data kommer in från den seriella enheten").
        writeRaw(masterFD, Array("hej\n".utf8))
        var received: [UInt8] = []
        for try await chunk in session.output {
            received.append(contentsOf: chunk)
            if received.count >= 4 { break }
        }
        XCTAssertEqual(String(decoding: received, as: UTF8.self), "hej\n")

        // session.send -> master ("data skickas UT till den seriella enheten").
        session.send("echo\n")
        var readBuf = [UInt8](repeating: 0, count: 64)
        // PTY-skrivningen ovan är asynkron ur NIOs event loop — en kort
        // pollningsloop istället för en fast sleep, samma stil som övriga
        // riktiga-nätverk-tester i den här sviten (LoopbackServer m.fl.
        // förlitar sig också på att data dyker upp inom kort, inte direkt).
        var total = 0
        for _ in 0..<50 {
            let n = readBuf.withUnsafeMutableBytes { buf in
                read(masterFD, buf.baseAddress, buf.count)
            }
            if n > 0 { total = n; break }
            try? await Task.sleep(nanoseconds: 20_000_000)
        }
        XCTAssertEqual(String(decoding: readBuf[0..<total], as: UTF8.self), "echo\n")

        await session.close()
    }

    func testUnsupportedBaudRateThrows() async throws {
        let (masterFD, slavePath) = try openPTYPair()
        defer { Darwin.close(masterFD) }

        do {
            _ = try await SerialSession.connect(config: SerialConfig(path: slavePath, baudRate: 42))
            XCTFail("förväntade unsupportedBaudRate")
        } catch SerialError.unsupportedBaudRate(42) {
            // förväntat
        }
    }

    func testCommonBaudRatesAllAccepted() async throws {
        for rate in SerialSession.commonBaudRates {
            let (masterFD, slavePath) = try openPTYPair()
            defer { Darwin.close(masterFD) }
            let session = try await SerialSession.connect(config: SerialConfig(path: slavePath, baudRate: rate))
            await session.close()
        }
    }
}
#endif
