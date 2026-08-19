import XCTest
@testable import SSHCore

private typealias Host = SSHCore.Host

final class SyncCryptoTests: XCTestCase {
    private func hex(_ bytes: [UInt8]) -> String {
        bytes.map { String(format: "%02x", $0) }.joined()
    }

    // Kända testvektorer för PBKDF2-HMAC-SHA256 (password="password", salt="salt").
    func testPBKDF2KnownAnswerVectors() {
        let pw = Array("password".utf8), salt = Array("salt".utf8)
        XCTAssertEqual(hex(SyncCrypto.pbkdf2SHA256(password: pw, salt: salt, iterations: 1, keyLength: 32)),
                       "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b")
        XCTAssertEqual(hex(SyncCrypto.pbkdf2SHA256(password: pw, salt: salt, iterations: 2, keyLength: 32)),
                       "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43")
        XCTAssertEqual(hex(SyncCrypto.pbkdf2SHA256(password: pw, salt: salt, iterations: 4096, keyLength: 32)),
                       "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a")
    }

    private func sampleState() -> SyncState {
        SyncState(hosts: [Host(alias: "web", hostName: "10.0.0.5", user: "deploy", tags: ["prod"])])
    }

    func testSealOpenRoundTrip() throws {
        let state = sampleState()
        // Färre iterationer i testet för fart; formatet bär iterationstalet.
        let blob = try SyncCrypto.seal(state, passphrase: "correct horse", iterations: 1000)
        let opened = try SyncCrypto.open(blob, passphrase: "correct horse")
        XCTAssertEqual(opened.hosts.first?.alias, "web")
    }

    func testWrongPassphraseFails() throws {
        let blob = try SyncCrypto.seal(sampleState(), passphrase: "rätt", iterations: 1000)
        XCTAssertThrowsError(try SyncCrypto.open(blob, passphrase: "fel")) {
            XCTAssertEqual($0 as? SyncCryptoError, .wrongPassphraseOrTampered)
        }
    }

    func testTamperIsDetected() throws {
        var blob = try SyncCrypto.seal(sampleState(), passphrase: "pw", iterations: 1000)
        blob[blob.count - 1] ^= 0xFF        // ändra sista chiffertext-byten
        XCTAssertThrowsError(try SyncCrypto.open(blob, passphrase: "pw")) {
            XCTAssertEqual($0 as? SyncCryptoError, .wrongPassphraseOrTampered)
        }
    }

    func testCiphertextLeaksNoPlaintext() throws {
        let blob = try SyncCrypto.seal(sampleState(), passphrase: "pw", iterations: 1000)
        let text = String(decoding: blob, as: UTF8.self)
        XCTAssertFalse(text.contains("10.0.0.5"))
        XCTAssertFalse(text.contains("deploy"))
        XCTAssertFalse(text.contains("web"))
    }

    // Två enheter synkar genom en KRYPTERAD delad fil och konvergerar.
    func testEncryptedProviderConverges() throws {
        let dir = NSTemporaryDirectory() + "bastion-enc-\(ProcessInfo.processInfo.processIdentifier)"
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let provider = EncryptedFolderSyncProvider(path: dir + "/shared.enc", passphrase: "delad-hemlis")
        let deviceA = HostStore(path: dir + "/a.json")
        let deviceB = HostStore(path: dir + "/b.json")
        // Egna, tomma snippet-databaser: testet handlar om krypteringen, och
        // den enda synkvägen tar båda.
        let snipsA = SnippetStore(path: dir + "/a-snippets.json")
        let snipsB = SnippetStore(path: dir + "/b-snippets.json")

        let h = Host(id: UUID(), alias: "nas", hostName: "10.0.0.2", user: "root")
        deviceA.upsert(h)
        try deviceA.sync(with: provider, snippets: snipsA)
        try deviceB.sync(with: provider, snippets: snipsB)
        XCTAssertEqual(deviceB.get(h.id)?.alias, "nas")

        // Fel lösenfras på en tredje enhet -> kan inte läsa.
        let wrong = EncryptedFolderSyncProvider(path: dir + "/shared.enc", passphrase: "gissning")
        XCTAssertThrowsError(try wrong.pull())
    }
}
