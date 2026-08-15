import NIOCore
import XCTest

@testable import SSHCore

/// En avvisad autentisering måste ALLTID avsluta anroparen — via `fatal`, inte
/// via att servern råkar stänga TCP-anslutningen.
///
/// Bakgrund (ROADMAP "Nästa steg" punkt 4): `bastion-cli.exe` med fel lösenord
/// hängde OÄNDLIGT på Windows mot en riktig sshd, medan exakt samma anrop
/// felade snabbt på Linux/macOS med `channelFailed("End of file")`. Felmeddelandet
/// var ledtråden: EOF är SERVERNS nedkoppling, inte vår egen auth-signal. Alla
/// tre kanalöppnarna (`execute()`, `openShell()`, `SFTPClient.open()`) använde
/// EN gemensam engångsspärr för både "får barn-kanalen skapas" och "vem avslutar
/// anroparen". Pipeline-uppslagningen (`channel.pipeline.handler(type:)`) svarar
/// praktiskt taget omedelbart — NIOSSHHandler ligger redan i pipelinen från
/// channelInitializer — så den vann alltid spärren, långt innan autentiseringen
/// var klar. Därefter kunde varken `fatal` eller `closeFuture` avsluta något,
/// och enda kvarvarande utvägen var att NIOSSH felade den föräldralösa
/// barn-promisen när kanalen stängdes. På Linux/macOS gjorde den det (därav
/// EOF-felet); på Windows gjorde den det inte, och anroparen väntade för alltid.
///
/// Testerna nedan låser fast att felet nu kommer från `fatal` — det TYPADE
/// `.authenticationFailed`, inte ett `.channelFailed(...)` som beror på att
/// motparten var vänlig nog att koppla ner. `signalFatal()` fullbordar
/// `fatal`-promisen INNAN den stänger kanalen, så ordningen är deterministisk.
/// Med den gamla enkelspärren gick de här igenom med `channelFailed("End of
/// file")` i stället, alltså exakt den plattformsberoende vägen.
final class AuthFailureTerminationTests: XCTestCase {
    private func makeSessionWithWrongPassword(port: Int) -> SSHSession {
        SSHSession(
            target: SSHTarget(host: "127.0.0.1", port: port, username: "tester"),
            auth: .password("fel lösenord"),
            knownHosts: KnownHosts(path: nil))
    }

    /// `execute()` — strömmen ska avslutas med `.authenticationFailed`.
    func testExecuteFailsWithTypedAuthenticationErrorNotServerEOF() async throws {
        let server = try LoopbackServer.start(password: "rätt lösenord")
        defer { server.shutdown() }

        let session = makeSessionWithWrongPassword(port: server.port)
        // connect() gör bara TCP + pipeline — auth-felet är asynkront och
        // dyker upp först vid första kanalöppningen, precis som ProxyJumpTests
        // redan dokumenterar.
        try await session.connect()

        // Timeout, inte en naken await: en regression HÄNGER (det är hela
        // buggen), och en hängd testsvit säger inget om vilket test som gick
        // sönder. 20s är väl tilltaget mot en loopback-server.
        let error: Error? = try await withTimeout(seconds: 20) {
            do {
                _ = try await session.run("whoami")
                return nil
            } catch {
                return error
            }
        }
        await session.close()

        guard let sshError = error as? SSHError else {
            XCTFail("förväntade ett SSHError, fick \(String(describing: error))")
            return
        }
        guard case .authenticationFailed = sshError else {
            XCTFail("förväntade .authenticationFailed, fick \(sshError)")
            return
        }
    }

    /// `openShell()` — samma invariant, men vägen dit går via ett
    /// `EventLoopPromise` i stället för en ström.
    func testOpenShellFailsWithTypedAuthenticationErrorNotServerEOF() async throws {
        let server = try LoopbackServer.start(password: "rätt lösenord")
        defer { server.shutdown() }

        let session = makeSessionWithWrongPassword(port: server.port)
        try await session.connect()

        let error: Error? = try await withTimeout(seconds: 20) {
            do {
                _ = try await session.openShell()
                return nil
            } catch {
                return error
            }
        }
        await session.close()

        guard let sshError = error as? SSHError else {
            XCTFail("förväntade ett SSHError, fick \(String(describing: error))")
            return
        }
        guard case .authenticationFailed = sshError else {
            XCTFail("förväntade .authenticationFailed, fick \(sshError)")
            return
        }
    }

    /// `SFTPClient.open()` — tredje kanalöppnaren, egen kodväg utanför
    /// `SSHSession`, samma spärrfel. Kastar `.authenticationFailed` oförändrat
    /// vidare (den befintliga `catch let error as SSHError`-grenen i
    /// `openChildChannel`), inte inpackat i ett nytt `.channelFailed(...)`.
    func testSFTPOpenFailsWithTypedAuthenticationErrorNotServerEOF() async throws {
        let server = try LoopbackServer.start(password: "rätt lösenord")
        defer { server.shutdown() }

        let session = makeSessionWithWrongPassword(port: server.port)
        try await session.connect()

        let error: Error? = try await withTimeout(seconds: 20) {
            do {
                _ = try await SFTPClient.open(on: session)
                return nil
            } catch {
                return error
            }
        }
        await session.close()

        guard let sshError = error as? SSHError else {
            XCTFail("förväntade ett SSHError, fick \(String(describing: error))")
            return
        }
        guard case .authenticationFailed = sshError else {
            XCTFail("förväntade .authenticationFailed, fick \(sshError)")
            return
        }
    }
}
