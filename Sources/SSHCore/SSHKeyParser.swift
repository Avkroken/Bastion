import Crypto
import Foundation

public enum SSHKeyError: Error, Sendable {
    case notOpenSSHFormat
    case unsupportedKeyType(String)
    case encrypted          // lösenfras-skyddad — stöds inte än
    case malformed
    /// Certifikatets inbäddade publika nyckel matchar inte den laddade
    /// privata nyckeln — de hör inte ihop (CodeRabbit-fynd, PR #93).
    case certificateKeyMismatch
}

/// Läser en privatnyckel i OpenSSH-format med standardens PEM-hölje,
/// den standard `ssh-keygen` skapar. Stöder okrypterade Ed25519-nycklar och
/// returnerar auth-metoden direkt. Krypterade nycklar (lösenfras) samt RSA/ECDSA
/// är nästa steg — vi kastar tydligt fel i stället för att gissa.
public enum OpenSSHPrivateKey {
    private static let pemLabel = ["OPENSSH", "PRIVATE", "KEY"].joined(separator: " ")

    public static func parse(_ pem: String) throws -> SSHAuth {
        let body = pem
            .split(whereSeparator: { $0 == "\n" || $0 == "\r" })
            .filter { !$0.hasPrefix("-----") }
            .joined()
        guard let data = Data(base64Encoded: body) else { throw SSHKeyError.notOpenSSHFormat }

        var r = ByteReader(data)
        guard (try? r.expect(Array("openssh-key-v1".utf8) + [0])) != nil else {
            throw SSHKeyError.notOpenSSHFormat
        }
        let cipher = try r.readStringUTF8()
        let kdf = try r.readStringUTF8()
        _ = try r.readString()            // kdfoptions
        guard try r.readU32() == 1 else { throw SSHKeyError.malformed }
        _ = try r.readString()            // publik nyckel

        let privSection = try r.readString()
        guard cipher == "none", kdf == "none" else { throw SSHKeyError.encrypted }

        var pr = ByteReader(Data(privSection))
        let check1 = try pr.readU32()
        let check2 = try pr.readU32()
        guard check1 == check2 else { throw SSHKeyError.malformed }   // fel lösenfras/korrupt

        let keyType = try pr.readStringUTF8()
        switch keyType {
        case "ssh-ed25519":
            _ = try pr.readString()           // publik nyckel (32 byte)
            let secret = try pr.readString()  // 64 byte: frö(32) || publik(32)
            guard secret.count == 64 else { throw SSHKeyError.malformed }
            return .ed25519Seed(Data(secret.prefix(32)))

        case "ecdsa-sha2-nistp256", "ecdsa-sha2-nistp384", "ecdsa-sha2-nistp521":
            let curve: ECDSACurve = keyType.hasSuffix("256") ? .p256 : keyType.hasSuffix("384") ? .p384 : .p521
            _ = try pr.readStringUTF8()       // kurvnamn ("nistp256" m.fl.)
            _ = try pr.readString()           // publik punkt Q (0x04 || X || Y)
            let d = try pr.readString()       // mpint: privat skalär
            return .ecdsa(curve: curve, scalar: Data(normalizeScalar(d, length: curve.scalarLength)))

        default:
            throw SSHKeyError.unsupportedKeyType(keyType)
        }
    }

    /// Normaliserar en SSH `mpint` (big-endian, minimal längd, en ledande
    /// 0x00-byte tillåten bara för teckendisambiguering) till exakt `length`
    /// byte — CryptoKits `rawRepresentation`-initierare kräver exakt längd,
    /// men mpint-kodningen kan vara kortare (litet tal) eller en byte längre
    /// (disambigueringsbyten).
    private static func normalizeScalar(_ bytes: [UInt8], length: Int) -> [UInt8] {
        var b = bytes
        while b.count > length, b.first == 0 { b.removeFirst() }
        if b.count < length { b = Array(repeating: 0, count: length - b.count) + b }
        return b
    }

    /// Läser och parsar en nyckelfil från disk.
    public static func load(path: String) throws -> SSHAuth {
        let pem = try String(contentsOfFile: path, encoding: .utf8)
        return try parse(pem)
    }

    /// Läser en privatnyckel + dess signerade OpenSSH-certifikat
    /// (`ssh-keygen -s` skriver typiskt `<nyckel>-cert.pub` bredvid
    /// `<nyckel>`) — kombinationen `SSHAuth.certificate` behöver för att
    /// erbjuda certifikatet men signera med den underliggande privata
    /// nyckeln (se `SSHUserAuth.swift`).
    ///
    /// Verifierar att certifikatets INBÄDDADE publika nyckel faktiskt
    /// matchar den laddade privata nyckeln (CodeRabbit-fynd, PR #93) —
    /// annars skulle en användare som råkar peka på en OMATCHAD nyckel/
    /// cert-kombination bara upptäcka det senare, som ett förvirrande
    /// SSH-autentiseringsfel, istället för ett tydligt fel direkt vid
    /// inläsning.
    public static func loadCertificate(keyPath: String, certPath: String) throws -> SSHAuth {
        guard case .ed25519Seed(let seed) = try load(path: keyPath) else {
            throw SSHKeyError.unsupportedKeyType("certifikatautentisering kräver en Ed25519-nyckel")
        }
        let certLine = try String(contentsOfFile: certPath, encoding: .utf8)
        let certificate = try OpenSSHCertificate.parse(certLine)
        let derivedPublicKey = try Curve25519.Signing.PrivateKey(rawRepresentation: seed).publicKey.rawRepresentation
        guard certificate.publicKey == derivedPublicKey else {
            throw SSHKeyError.certificateKeyMismatch
        }
        return .certificate(seed: seed, certificateLine: certLine)
    }

    /// Bygger en okrypterad Ed25519-nyckel i OpenSSH-filformat (samma format
    /// `ssh-keygen` skriver) — inversen av `parse` ovan, byte för byte samma
    /// struktur. Enda vägen att verifiera en encoder utan en referens-
    /// implementation att jämföra mot är att bevisa att den egna decodern
    /// (redan skriven, redan bevisad mot riktiga `ssh-keygen`-nycklar) läser
    /// tillbaka exakt samma frö — se `SSHKeyParserTests`s round-trip-test.
    public static func export(seed: Data, comment: String = "") throws -> String {
        guard seed.count == 32 else { throw SSHKeyError.malformed }
        let privateKey = try Curve25519.Signing.PrivateKey(rawRepresentation: seed)
        let publicKey = Array(privateKey.publicKey.rawRepresentation)

        var pub = ByteWriter()
        pub.writeString(Array("ssh-ed25519".utf8))
        pub.writeString(publicKey)

        var priv = ByteWriter()
        let checkint = UInt32.random(in: .min ... .max)
        priv.writeU32(checkint)
        priv.writeU32(checkint)
        priv.writeString(Array("ssh-ed25519".utf8))
        priv.writeString(publicKey)
        priv.writeString(Array(seed) + publicKey)
        priv.writeString(Array(comment.utf8))
        // Utfyllnad till ett multipel av 8 (blockstorleken för "none"-chiffret)
        // — OpenSSH-formatet kräver 1,2,3,... som utfyllnadsbytes, inte nollor.
        var padByte: UInt8 = 1
        while priv.bytes.count % 8 != 0 {
            priv.bytes.append(padByte)
            padByte += 1
        }

        var whole = ByteWriter()
        whole.bytes.append(contentsOf: Array("openssh-key-v1".utf8) + [0])
        whole.writeString(Array("none".utf8))       // ciphername
        whole.writeString(Array("none".utf8))       // kdfname
        whole.writeString([])                       // kdfoptions
        whole.writeU32(1)                           // antal nycklar
        whole.writeString(pub.bytes)
        whole.writeString(priv.bytes)

        let base64 = Data(whole.bytes).base64EncodedString()
        let lines = stride(from: 0, to: base64.count, by: 70).map { start -> Substring in
            let s = base64.index(base64.startIndex, offsetBy: start)
            let e = base64.index(s, offsetBy: 70, limitedBy: base64.endIndex) ?? base64.endIndex
            return base64[s..<e]
        }
        return "-----BEGIN \(pemLabel)-----\n"
            + lines.joined(separator: "\n")
            + "\n-----END \(pemLabel)-----\n"
    }
}

/// Minimal läsare för SSH:s binära wire-format (uint32-längd + bytes, big-endian).
private struct ByteReader {
    private let bytes: [UInt8]
    private var offset = 0
    init(_ data: Data) { bytes = Array(data) }

    mutating func readU32() throws -> UInt32 {
        guard offset + 4 <= bytes.count else { throw SSHKeyError.malformed }
        defer { offset += 4 }
        return (UInt32(bytes[offset]) << 24) | (UInt32(bytes[offset + 1]) << 16)
            | (UInt32(bytes[offset + 2]) << 8) | UInt32(bytes[offset + 3])
    }

    mutating func readBytes(_ n: Int) throws -> [UInt8] {
        guard n >= 0, offset + n <= bytes.count else { throw SSHKeyError.malformed }
        defer { offset += n }
        return Array(bytes[offset..<offset + n])
    }

    mutating func readString() throws -> [UInt8] {
        try readBytes(Int(try readU32()))
    }

    mutating func readStringUTF8() throws -> String {
        String(decoding: try readString(), as: UTF8.self)
    }

    mutating func expect(_ magic: [UInt8]) throws {
        guard try readBytes(magic.count) == magic else { throw SSHKeyError.notOpenSSHFormat }
    }
}

/// Inversen av `ByteReader` — bygger SSH:s binära wire-format (uint32-längd
/// + bytes, big-endian).
private struct ByteWriter {
    var bytes: [UInt8] = []

    mutating func writeU32(_ value: UInt32) {
        bytes.append(UInt8((value >> 24) & 0xFF))
        bytes.append(UInt8((value >> 16) & 0xFF))
        bytes.append(UInt8((value >> 8) & 0xFF))
        bytes.append(UInt8(value & 0xFF))
    }

    mutating func writeString(_ value: [UInt8]) {
        writeU32(UInt32(value.count))
        bytes.append(contentsOf: value)
    }
}
