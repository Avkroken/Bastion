import Foundation

/// Vart vi ansluter.
public struct SSHTarget: Sendable {
    public var host: String
    public var port: Int
    public var username: String

    public init(host: String, port: Int = 22, username: String) {
        self.host = host
        self.port = port
        self.username = username
    }
}

/// De ECDSA-kurvor swift-nio-ssh stödjer för klientautentisering. RSA stöds
/// INTE alls av swift-nio-ssh på klientsidan (bekräftat i `NIOSSHPrivateKey`s
/// källa) — bara Ed25519 och dessa tre NIST-kurvor.
public enum ECDSACurve: Sendable, Equatable {
    case p256, p384, p521

    /// Förväntad längd (byte) på den råa privata skalären i OpenSSH:s
    /// nyckelformat (mpint, vänsterutfylld till denna längd).
    var scalarLength: Int {
        switch self {
        case .p256: return 32
        case .p384: return 48
        case .p521: return 66
        }
    }
}

/// Autentiseringsmetod. Lösenord är fullt implementerat. Publik nyckel stöds
/// för råa Ed25519-frön (32 byte) samt okrypterade ECDSA-nycklar (P256/P384/
/// P521) via OpenSSH-filparsning (`~/.ssh/id_ed25519`/`id_ecdsa`) — se
/// `SSHKeyParser`/`SSHUserAuth`. RSA-nycklar och lösenfrasskyddade nycklar
/// stöds inte än (`SSHKeyError.unsupportedKeyType`/`.encrypted`).
public enum SSHAuth: Sendable {
    case password(String)
    case ed25519Seed(Data)
    /// Rå privat ECDSA-skalär, vänsterutfylld till `curve.scalarLength` byte.
    case ecdsa(curve: ECDSACurve, scalar: Data)
    /// OpenSSH-certifikatautentisering: signerar med den råa Ed25519-fröet
    /// (`seed`, samma som `.ed25519Seed`) men erbjuder servern CERTIFIKATET
    /// (`certificateLine`, en hel `type base64 kommentar`-rad som en
    /// `-cert.pub`-fil) som "publik nyckel" istället för den bara nyckeln —
    /// servern validerar CA-signaturen + giltighet (se `SSHUserAuth.swift`).
    case certificate(seed: Data, certificateLine: String)
    /// Nyckel i Secure Enclave. `stored` är den ogenomskinliga
    /// representationen från `SecureEnclaveKey.generate()` — INTE den
    /// privata nyckeln, som aldrig lämnar kretsen och inte går att
    /// serialisera. Se `SecureEnclaveKey`.
    ///
    /// Bara meningsfull på Apple-plattformar; på Linux finns ingen krets
    /// att läsa den med, och auth misslyckas med ett tydligt fel i
    /// stället för att låtsas.
    case secureEnclave(stored: Data)
}

/// En bit utdata från fjärrkommandot.
public struct SSHChunk: Sendable {
    public enum Stream: Sendable { case stdout, stderr }
    public let stream: Stream
    public let bytes: [UInt8]

    public var text: String { String(decoding: bytes, as: UTF8.self) }
}

/// Värdnyckel-fingeravtryck som vi sett vid anslutning (TOFU-underlag för UI:t).
public struct HostKeyInfo: Sendable {
    public let sha256Fingerprint: String
    public let keyType: String
}

public enum SSHError: Error, Sendable {
    case connectionFailed(String)
    case authenticationFailed
    case channelFailed(String)
    case hostKeyRejected(HostKeyInfo)
    case remoteExit(status: Int)
}
