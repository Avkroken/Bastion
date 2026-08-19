import Foundation
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

    // MARK: - Iterationsgränser

    /// Talet i kuvertet är angriparkontrollerat: filen kommer per design från
    /// en obetrodd mapp. Utan övre gräns kan några hundra byte begära uppemot
    /// 4,3 miljarder PBKDF2-rundor, och härledningen körs INNAN AEAD hinner
    /// avvisa filen — timmar av CPU per synkförsök, vilket på iOS betyder en
    /// app som hänger tills watchdogen dödar den.
    ///
    /// Testet mäter TIDEN, inte bara att ett fel kastas: kontrollen måste
    /// ligga före `deriveKey`, annars är felet korrekt men skadan redan skedd.
    func testAnAbsurdIterationCountIsRejectedBeforeAnyDerivationRuns() throws {
        var envelope = try SyncCrypto.seal(SyncState(), passphrase: "hemlig")
        // Skriv över iterationsfältet med UInt32.max.
        let offset = SyncCrypto.magic.count
        envelope.replaceSubrange(offset..<(offset + 4), with: [0xFF, 0xFF, 0xFF, 0xFF])

        let start = ContinuousClock.now
        XCTAssertThrowsError(try SyncCrypto.open(envelope, passphrase: "hemlig")) { error in
            XCTAssertEqual(error as? SyncCryptoError, .badFormat)
        }
        XCTAssertLessThan(
            ContinuousClock.now - start, .seconds(2),
            "avvisandet måste ske FÖRE nyckelhärledningen — annars har angriparen "
                + "redan fått betalt i CPU-tid")
    }

    /// Motsatsen: ett kuvert sparat med en absurt SVAG härledning ska inte
    /// heller accepteras, så ingen kan göra en senare bruteforce billig genom
    /// att skriva `iterations: 1`.
    func testAnAbsurdlyWeakIterationCountIsRejected() throws {
        var envelope = try SyncCrypto.seal(SyncState(), passphrase: "hemlig")
        let offset = SyncCrypto.magic.count
        envelope.replaceSubrange(offset..<(offset + 4), with: [0, 0, 0, 1])
        XCTAssertThrowsError(try SyncCrypto.open(envelope, passphrase: "hemlig")) { error in
            XCTAssertEqual(error as? SyncCryptoError, .badFormat)
        }
    }

    /// Gränserna får inte vara så snäva att de avvisar riktiga kuvert.
    /// Standardvärdet, och båda ändpunkterna, ska gå igenom.
    func testTheDefaultAndBothBoundsAreAccepted() throws {
        for iterations in [SyncCrypto.minIterations, SyncCrypto.defaultIterations] {
            let envelope = try SyncCrypto.seal(
                SyncState(), passphrase: "hemlig", iterations: iterations)
            XCTAssertNoThrow(
                try SyncCrypto.open(envelope, passphrase: "hemlig"),
                "iterations = \(iterations) ska accepteras")
        }
    }

    /// Värdena MÅSTE vara samma som LinuxApps (`sync_crypto.rs`). Går de isär
    /// blir ett kuvert som ena plattformen skrivit oläsbart på den andra —
    /// en synk som slutar fungera utan att någon ändrat något.
    func testTheBoundsMatchTheOnesLinuxAppUses() {
        XCTAssertEqual(SyncCrypto.minIterations, 1_000)
        XCTAssertEqual(SyncCrypto.maxIterations, 10_000_000)
        XCTAssertEqual(SyncCrypto.defaultIterations, 210_000)
    }

    // MARK: - Storlekstak på synkfilen

    /// Filen kommer från en mapp vi INTE kontrollerar. Utan tak allokerar
    /// `Data(contentsOf:)` hela längden innan något tittat på innehållet — på
    /// en telefon en omedelbar OOM-död.
    ///
    /// Testet skriver en fil som är EN byte över taket, inte flera gigabyte:
    /// det är gränsen som ska bevisas, och en gigabytefil i en testsvit vore
    /// ett självmål.
    func testAFileOverTheCapIsRefusedInsteadOfReadIntoMemory() throws {
        let dir = NSTemporaryDirectory() + "bastion-cap-\(UUID().uuidString)"
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let path = dir + "/state.json"
        try Data(repeating: 0x78, count: SyncFileLimits.maxBytes + 1)
            .write(to: URL(fileURLWithPath: path))

        XCTAssertThrowsError(try SyncFileLimits.read(path)) { error in
            XCTAssertEqual(error as? SyncFileError, .tooLarge(maxBytes: SyncFileLimits.maxBytes))
        }

        // Och providern ska föra felet vidare, inte svälja det och låtsas att
        // mappen var tom — "inget att synka" och "någon la en gigabytefil
        // här" är olika saker.
        let provider = EncryptedFolderSyncProvider(path: path, passphrase: "hemlig")
        XCTAssertThrowsError(try provider.pull())
    }

    /// Taket får inte avvisa något verkligt. En fil precis PÅ gränsen ska
    /// läsas — annars är det inte ett skydd utan en godtycklig begränsning.
    func testAFileExactlyAtTheCapIsStillRead() throws {
        let dir = NSTemporaryDirectory() + "bastion-cap-\(UUID().uuidString)"
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let path = dir + "/stor.bin"
        try Data(repeating: 0x79, count: SyncFileLimits.maxBytes)
            .write(to: URL(fileURLWithPath: path))

        XCTAssertEqual(try SyncFileLimits.read(path).count, SyncFileLimits.maxBytes)
    }

    /// Ett vanligt, krypterat tillstånd ska gå igenom precis som förut.
    func testANormalEnvelopeRoundTripsThroughTheCappedRead() throws {
        let dir = NSTemporaryDirectory() + "bastion-cap-\(UUID().uuidString)"
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let provider = EncryptedFolderSyncProvider(path: dir + "/state.enc", passphrase: "hemlig")

        var state = SyncState()
        state.hosts.append(Host(alias: "a", hostName: "10.0.0.1", user: "u"))
        try provider.push(state)
        let back = try provider.pull()
        XCTAssertEqual(back?.hosts.count, 1)
    }

    /// Värdet MÅSTE vara samma som LinuxApps `MAX_SYNC_FILE_BYTES`. Går de
    /// isär accepterar ena plattformen en fil den andra vägrar, vilket ser ut
    /// som en synk som fungerar på en enhet men inte på nästa.
    func testTheCapMatchesTheOneLinuxAppUses() {
        XCTAssertEqual(SyncFileLimits.maxBytes, 64 * 1024 * 1024)
    }
}
