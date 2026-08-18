import Crypto
import Foundation
import XCTest
@testable import SSHCore

/// `OpenSSHPrivateKey.parse` läser användarens privata nyckel. Den hade
/// inget test alls, och felen den ska ge är inte kosmetiska: en krypterad
/// nyckel som tyst tolkas fel, eller en ECDSA-nyckel som accepteras som
/// Ed25519, ger en anslutning som misslyckas utan att säga varför.
///
/// Nycklarna nedan är genererade med riktiga `ssh-keygen` (OpenSSH 9.6)
/// 2026-08-18 och är ENGÅNGSNYCKLAR som aldrig använts mot någon server —
/// de finns bara som testdata.
final class SSHKeyParserTests: XCTestCase {

    /// `ssh-keygen -t ed25519 -N "" -C parser@bastion`
    private static let ed25519PEM = """
-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACDjCnM3LvREj2+qPBcFWfii0iyG7rSWxxG+v5/8EyHHsgAAAJjO+J2Uzvid
lAAAAAtzc2gtZWQyNTUxOQAAACDjCnM3LvREj2+qPBcFWfii0iyG7rSWxxG+v5/8EyHHsg
AAAEC+PxKeVdQ3px1dtQAIzGPwD+O+juzbJW28zRxU6+zWMuMKczcu9ESPb6o8FwVZ+KLS
LIbutJbHEb6/n/wTIceyAAAADnBhcnNlckBiYXN0aW9uAQIDBAUGBw==
-----END OPENSSH PRIVATE KEY-----
"""

    /// Fröet ur samma fil, uträknat oberoende av parsern (base64 av de
    /// första 32 byten i den privata sektionen). Att jämföra mot ETT
    /// EXAKT värde är poängen — en parser som ger "någon nyckel" är
    /// värdelös, den ska ge RÄTT nyckel.
    private static let expectedSeedBase64 = "vj8SnlXUN6cdXbUACMxj8A/jvo7s2yVtvM0cVOvs1jI="

    /// `ssh-keygen -t ed25519 -N hemlig` — lösenfrasskyddad.
    private static let encryptedPEM = """
-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABBsflX39+
s4EDQq1fI0s7ldAAAAGAAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5AAAAIIcd3KwHjoRx9ovp
6w2KrThZRJVLUQKUJZusx/2pmUKKAAAAoMAWzZWFeXs+hX81L+WVcy5vykkbqjES6mpikk
gzi5fA6nRfWdgiwCYTJeemkm8XkJhZBf5MFUFAYm3kanLdfjGkM7JBKBnjSTP3F7kW6kdT
juqhrUyu0cAEgM06L9LRhk1lOfqZwNu8q6JZ4Xf2kwxaXoE2y75giVZ8noXdVKZn42zeUM
pH+sdBSRKUOebwNIybbi+aULdNrWLUFAwet8o=
-----END OPENSSH PRIVATE KEY-----
"""

    /// `ssh-keygen -t ecdsa -N ""` — giltig nyckel, men inte en typ
    /// parsern hanterar.
    private static let ecdsaPEM = """
-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAaAAAABNlY2RzYS
1zaGEyLW5pc3RwMjU2AAAACG5pc3RwMjU2AAAAQQSgZfmrm5vzWFq/7pg1PCXzaMjMfZL3
YQ9hTclvSZXDmYOtnWmuXggfRv4dIrAsuUHA321FNWkouaBWSunjWfEDAAAAqJSyaPiUsm
j4AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBKBl+aubm/NYWr/u
mDU8JfNoyMx9kvdhD2FNyW9JlcOZg62daa5eCB9G/h0isCy5QcDfbUU1aSi5oFZK6eNZ8Q
MAAAAhAKigp47mwEXwbyOvbe8MvWJPeYc7XUdCPVHr0mVHWRyeAAAADWVjZHNhQGJhc3Rp
b24BAg==
-----END OPENSSH PRIVATE KEY-----
"""

    /// Den enda vägen som ska lyckas — och den ska ge exakt rätt
    /// nyckelmaterial, inte bara något som ser ut som en nyckel.
    func testParsesAnUnencryptedEd25519KeyToTheExactSeed() throws {
        let auth = try OpenSSHPrivateKey.parse(Self.ed25519PEM)
        guard case .ed25519Seed(let seed) = auth else {
            return XCTFail("förväntade .ed25519Seed, fick \(auth)")
        }
        XCTAssertEqual(seed.count, 32, "ett Ed25519-frö är alltid 32 byte")
        XCTAssertEqual(seed.base64EncodedString(), Self.expectedSeedBase64)

        // Fröet ska gå att använda — inte bara ha rätt längd.
        XCTAssertNoThrow(try Curve25519.Signing.PrivateKey(rawRepresentation: seed))
    }

    /// En lösenfrasskyddad nyckel ska ge `.encrypted`, inte `.malformed`.
    /// Skillnaden är vad användaren ska göra: skriva in en fras, eller
    /// leta efter en trasig fil.
    func testAnEncryptedKeyIsReportedAsEncryptedNotMalformed() {
        XCTAssertThrowsError(try OpenSSHPrivateKey.parse(Self.encryptedPEM)) { error in
            XCTAssertEqual(error as? SSHKeyError, .encrypted)
        }
    }

    /// En ECDSA-nyckel är fullt giltig — den stöds bara inte här. Felet
    /// ska säga vilken typ det var, så meddelandet blir användbart.
    func testAnUnsupportedKeyTypeNamesTheTypeItFound() {
        XCTAssertThrowsError(try OpenSSHPrivateKey.parse(Self.ecdsaPEM)) { error in
            guard case .unsupportedKeyType(let type)? = error as? SSHKeyError else {
                return XCTFail("förväntade .unsupportedKeyType, fick \(error)")
            }
            XCTAssertTrue(type.contains("ecdsa"), "typen ska stå i felet: \(type)")
        }
    }

    /// Allt som inte ens är OpenSSH-format ska falla på formatkontrollen,
    /// inte längre in där felet blir svårare att tyda.
    func testNonOpenSSHInputIsRejectedAtTheFormatCheck() {
        for junk in [
            "",
            "inte en nyckel alls",
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEow==\n-----END RSA PRIVATE KEY-----",
        ] {
            XCTAssertThrowsError(try OpenSSHPrivateKey.parse(junk), "skulle avvisats: \(junk)")
        }
    }

    /// En avhuggen nyckel ska ge ett fel, inte en halv nyckel och inte en
    /// krasch. Läsaren går över bytegränser om den inte kontrollerar.
    func testATruncatedKeyFailsInsteadOfReturningHalfAKey() {
        let lines = Self.ed25519PEM.split(separator: "\n").map(String.init)
        // Behåll höljet men klipp bort nästan hela kroppen: header, EN
        // kroppsrad, footer.
        var kept: [String] = []
        if let header = lines.first { kept.append(header) }
        if lines.count > 2 { kept.append(lines[1]) }
        if let footer = lines.last { kept.append(footer) }
        let truncated = kept.joined(separator: "\n")
        XCTAssertThrowsError(try OpenSSHPrivateKey.parse(truncated))
    }

    /// Export ska gå att läsa tillbaka. Utan det kan appen skriva nycklar
    /// den själv inte kan öppna.
    func testExportedKeyRoundTripsThroughTheParser() throws {
        let seed = Curve25519.Signing.PrivateKey().rawRepresentation
        let pem = try OpenSSHPrivateKey.export(seed: seed, comment: "round@trip")
        XCTAssertTrue(pem.contains("BEGIN OPENSSH PRIVATE KEY"))

        guard case .ed25519Seed(let back) = try OpenSSHPrivateKey.parse(pem) else {
            return XCTFail("exporterad nyckel gick inte att läsa tillbaka")
        }
        XCTAssertEqual(back, seed)
    }
}
