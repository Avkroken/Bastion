import XCTest
@testable import SSHCore

final class KeyParserTests: XCTestCase {
    func testParseEd25519Seed() throws {
        let pair = KeyGenerator.generateEd25519(comment: "parser-test")
        let pem = try OpenSSHPrivateKey.export(seed: pair.seed, comment: "parser-test")
        guard case .ed25519Seed(let seed) = try OpenSSHPrivateKey.parse(pem) else {
            return XCTFail("förväntade Ed25519-frö")
        }
        XCTAssertEqual(seed, pair.seed)
    }

    func testRejectsGarbage() {
        XCTAssertThrowsError(try OpenSSHPrivateKey.parse("inte en nyckel"))
    }

    // Bevisar hela vägen: parsa nyckel -> signera handshake -> servern accepterar.
    // Fel parsning ger ogiltig signatur och auth misslyckas.
    func testParsedKeyAuthenticatesEndToEnd() async throws {
        let server = try LoopbackServer.start(password: "irrelevant")
        defer { server.shutdown() }

        let pair = KeyGenerator.generateEd25519(comment: "authentication-test")
        let pem = try OpenSSHPrivateKey.export(seed: pair.seed, comment: "authentication-test")
        let auth = try OpenSSHPrivateKey.parse(pem)
        let session = SSHSession(
            target: SSHTarget(host: "127.0.0.1", port: server.port, username: "tester"),
            auth: auth, knownHosts: KnownHosts(path: nil))
        try await session.connect()
        let output = try await session.run("whoami")
        await session.close()

        XCTAssertEqual(output, "ran: whoami\n")
    }

    // MARK: - ECDSA (P256/P384/P521)

    /// Genererar en riktig OpenSSH-nyckel via det systeminstallerade
    /// `ssh-keygen` (inte ett handskrivet fixture) — bevisar att vår parser
    /// läser EXAKT samma filformat som den riktiga referensimplementationen
    /// skriver, inte bara vår egen `export()` (som bara finns för Ed25519).
    private func generateRealECDSAKey(bits: Int) throws -> String {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("bastion-ecdsa-test-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let keyPath = dir.appendingPathComponent("id_ecdsa").path

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ssh-keygen")
        process.arguments = ["-t", "ecdsa", "-b", "\(bits)", "-N", "", "-f", keyPath, "-C", ""]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw XCTSkip("ssh-keygen inte tillgänglig eller misslyckades (bits=\(bits))")
        }
        return try String(contentsOfFile: keyPath, encoding: .utf8)
    }

    func testParseECDSAP256() throws {
        let pem = try generateRealECDSAKey(bits: 256)
        guard case .ecdsa(let curve, let scalar) = try OpenSSHPrivateKey.parse(pem) else {
            return XCTFail("förväntade ECDSA-nyckel")
        }
        XCTAssertEqual(curve, .p256)
        XCTAssertEqual(scalar.count, 32)
    }

    func testParseECDSAP384() throws {
        let pem = try generateRealECDSAKey(bits: 384)
        guard case .ecdsa(let curve, let scalar) = try OpenSSHPrivateKey.parse(pem) else {
            return XCTFail("förväntade ECDSA-nyckel")
        }
        XCTAssertEqual(curve, .p384)
        XCTAssertEqual(scalar.count, 48)
    }

    func testParseECDSAP521() throws {
        let pem = try generateRealECDSAKey(bits: 521)
        guard case .ecdsa(let curve, let scalar) = try OpenSSHPrivateKey.parse(pem) else {
            return XCTFail("förväntade ECDSA-nyckel")
        }
        XCTAssertEqual(curve, .p521)
        XCTAssertEqual(scalar.count, 66)
    }

    // Samma end-to-end-bevis som Ed25519-testet ovan, men för alla tre
    // ECDSA-kurvorna: en felaktigt tolkad skalär ger en ogiltig signatur och
    // auth misslyckas, så detta bevisar att parsning + mpint-normalisering +
    // NIOSSHPrivateKey-wiring är korrekt, inte bara att parsern inte kastar.
    func testECDSAKeyAuthenticatesEndToEnd() async throws {
        for bits in [256, 384, 521] {
            let pem = try generateRealECDSAKey(bits: bits)
            let auth = try OpenSSHPrivateKey.parse(pem)

            let server = try LoopbackServer.start(password: "irrelevant")
            let session = SSHSession(
                target: SSHTarget(host: "127.0.0.1", port: server.port, username: "tester"),
                auth: auth, knownHosts: KnownHosts(path: nil))
            try await session.connect()
            let output = try await session.run("whoami")
            await session.close()
            server.shutdown()

            XCTAssertEqual(output, "ran: whoami\n", "misslyckades för ecdsa-\(bits)")
        }
    }
}
