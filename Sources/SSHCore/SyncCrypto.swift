import Crypto
import Foundation

public enum SyncCryptoError: Error, Sendable, Equatable {
    case badFormat
    case wrongPassphraseOrTampered
}

/// End-to-end-kryptering av synktillståndet. Allt krypteras på enheten innan det
/// lämnar den, så vilken molntjänst filen än hamnar i (iCloud/Dropbox/Google/
/// OneDrive) ser bara chiffertext. Nyckeln härleds ur en lösenfras med
/// PBKDF2-HMAC-SHA256 (arbetsfaktor mot brute force), och nyttolasten skyddas
/// med AES-256-GCM (autentiserad — manipulation upptäcks).
///
/// Kuvertformat: "BSYNC1" | iterationer(UInt32 BE) | salt(16) | AES-GCM combined.
public enum SyncCrypto {
    static let magic = Array("BSYNC1".utf8)
    public static let defaultIterations = 210_000
    static let saltLength = 16

    /// Gränser för `iterations` LÄST UR EN FIL. Kuvertet kommer per design
    /// från en obetrodd mapp (en delad Dropbox/Drive/iCloud-katalog), så
    /// talet är angriparkontrollerat.
    ///
    /// Utan övre gräns kan en planterad fil på några hundra byte begära
    /// uppemot 4,3 miljarder PBKDF2-rundor, och härledningen körs INNAN
    /// AEAD-autentiseringen hunnit avvisa filen — alltså timmar av CPU per
    /// synkförsök, om och om igen. På iOS betyder det en app som hänger
    /// tills systemets watchdog dödar den.
    ///
    /// Den undre gränsen skyddar mot motsatsen: ett kuvert sparat med
    /// `iterations: 1` av någon som vill göra en senare bruteforce billig.
    ///
    /// LinuxApp har haft de här gränserna sedan `sync_crypto.rs` skrevs, med
    /// exakt samma resonemang. Den här sidan saknade dem — samma sorts
    /// asymmetri som lät `forwardAgent` finnas på en plattform och inte den
    /// andra. Värdena hålls medvetet identiska.
    static let minIterations = 1_000
    static let maxIterations = 10_000_000

    public static func seal(_ state: SyncState, passphrase: String,
                            iterations: Int = SyncCrypto.defaultIterations) throws -> Data {
        let salt = randomBytes(saltLength)
        let key = deriveKey(passphrase: passphrase, salt: salt, iterations: iterations)
        let plaintext = try JSONEncoder().encode(state)
        let sealed = try AES.GCM.seal(plaintext, using: key)
        guard let combined = sealed.combined else { throw SyncCryptoError.badFormat }

        var out = Data(magic)
        out.append(contentsOf: uint32BE(UInt32(iterations)))
        out.append(contentsOf: salt)
        out.append(combined)
        return out
    }

    public static func open(_ data: Data, passphrase: String) throws -> SyncState {
        let header = magic.count + 4 + saltLength
        guard data.count > header, Array(data.prefix(magic.count)) == magic else {
            throw SyncCryptoError.badFormat
        }
        let bytes = Array(data)
        let iterations = Int(readUInt32BE(Array(bytes[magic.count..<magic.count + 4])))
        // Kontrolleras FÖRE `deriveKey` — hela poängen är att aldrig starta
        // en härledning vars kostnad en angripare valt.
        guard (minIterations...maxIterations).contains(iterations) else {
            throw SyncCryptoError.badFormat
        }
        let salt = Array(bytes[magic.count + 4..<header])
        let combined = Data(bytes[header...])

        let key = deriveKey(passphrase: passphrase, salt: salt, iterations: iterations)
        do {
            let box = try AES.GCM.SealedBox(combined: combined)
            let plaintext = try AES.GCM.open(box, using: key)
            return try JSONDecoder().decode(SyncState.self, from: plaintext)
        } catch is CryptoKitError {
            throw SyncCryptoError.wrongPassphraseOrTampered
        }
    }

    // MARK: - Nyckelhärledning

    static func deriveKey(passphrase: String, salt: [UInt8], iterations: Int) -> SymmetricKey {
        SymmetricKey(data: pbkdf2SHA256(
            password: Array(passphrase.utf8), salt: salt, iterations: iterations, keyLength: 32))
    }

    /// PBKDF2-HMAC-SHA256 (RFC 8018). Verifierad mot kända testvektorer i testerna.
    static func pbkdf2SHA256(password: [UInt8], salt: [UInt8], iterations: Int, keyLength: Int) -> [UInt8] {
        let key = SymmetricKey(data: password)
        let hLen = 32
        let blocks = (keyLength + hLen - 1) / hLen
        var derived = [UInt8]()
        derived.reserveCapacity(blocks * hLen)

        for block in 1...max(1, blocks) {
            var salted = salt
            salted.append(contentsOf: uint32BE(UInt32(block)))
            var u = Array(HMAC<SHA256>.authenticationCode(for: salted, using: key))
            var t = u
            if iterations > 1 {
                for _ in 1..<iterations {
                    u = Array(HMAC<SHA256>.authenticationCode(for: u, using: key))
                    for i in 0..<hLen { t[i] ^= u[i] }
                }
            }
            derived.append(contentsOf: t)
        }
        return Array(derived.prefix(keyLength))
    }

    // MARK: - Hjälpare

    static func randomBytes(_ n: Int) -> [UInt8] {
        var g = SystemRandomNumberGenerator()
        return (0..<n).map { _ in UInt8.random(in: 0...255, using: &g) }
    }

    static func uint32BE(_ v: UInt32) -> [UInt8] {
        [UInt8(v >> 24 & 0xFF), UInt8(v >> 16 & 0xFF), UInt8(v >> 8 & 0xFF), UInt8(v & 0xFF)]
    }

    static func readUInt32BE(_ b: [UInt8]) -> UInt32 {
        (UInt32(b[0]) << 24) | (UInt32(b[1]) << 16) | (UInt32(b[2]) << 8) | UInt32(b[3])
    }
}

/// Krypterad variant av `FolderSyncProvider`: samma mapp-transport, men filen är
/// AES-GCM-krypterad med en lösenfras. Det här är den man vill använda mot en
/// tredjeparts molnmapp.
public struct EncryptedFolderSyncProvider: SyncProvider {
    private let path: String
    private let passphrase: String

    public init(path: String, passphrase: String) {
        self.path = (path as NSString).expandingTildeInPath
        self.passphrase = passphrase
    }

    public func pull() throws -> SyncState? {
        guard FileManager.default.fileExists(atPath: path) else { return nil }
        // Samma tak som den okrypterade transporten. Här spelar det ännu
        // större roll: kuvertet är per definition obetrott innehåll från en
        // mapp vi inte kontrollerar.
        return try SyncCrypto.open(SyncFileLimits.read(path), passphrase: passphrase)
    }

    public func push(_ state: SyncState) throws {
        let dir = (path as NSString).deletingLastPathComponent
        try FileManager.default.createDirectory(
            atPath: dir, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        try SyncCrypto.seal(state, passphrase: passphrase).write(to: URL(fileURLWithPath: path))
    }
}
