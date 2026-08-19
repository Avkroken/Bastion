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

    /// `ssh-keygen -t ecdsa -N ""` — P256, en av de tre kurvor parsern
    /// faktiskt stöder.
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

    /// `ssh-keygen -t rsa -b 2048 -N "" -C parser@bastion` — giltig
    /// nyckel av en typ parsern medvetet INTE hanterar (se ROADMAP.md
    /// "Uppskjutet med avsikt"). Engångsnyckel, aldrig använd mot någon
    /// server.
    private static let rsaPEM = """
-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAABFwAAAAdzc2gtcn
NhAAAAAwEAAQAAAQEAvdRhixD4HEFQ2rPp02zyiEUmGYwOymaSo82dg1Cvb0+JUJi1QpU+
uHku3TFp9suIob16txec2qVXluykDuJ5U28vSdjylde4Huk1QrE+r8jWIyrgbycUJpZAbD
FyKp4UueuM85gCRPdz2v19n9XbsGmdXtGie0sc0y0wiKaa2abZN6ZOMIachIEEoDNZ2f/c
fm3A2qNCCYLTY1RyHzcXJIeYPeeVbCkn98jNDHbRinDaHEFrGy+1WfY8H9bY/PDHyGlLDh
KhhHWTt9eQNXoSAYaLGrFn3eqwoDQA6FvyhDngELm0d62DrsPmQIqUO8pfJu3k579eI3Ec
wD2LR9QlvwAAA8idU/SrnVP0qwAAAAdzc2gtcnNhAAABAQC91GGLEPgcQVDas+nTbPKIRS
YZjA7KZpKjzZ2DUK9vT4lQmLVClT64eS7dMWn2y4ihvXq3F5zapVeW7KQO4nlTby9J2PKV
17ge6TVCsT6vyNYjKuBvJxQmlkBsMXIqnhS564zzmAJE93Pa/X2f1duwaZ1e0aJ7SxzTLT
CIpprZptk3pk4whpyEgQSgM1nZ/9x+bcDao0IJgtNjVHIfNxckh5g955VsKSf3yM0MdtGK
cNocQWsbL7VZ9jwf1tj88MfIaUsOEqGEdZO315A1ehIBhosasWfd6rCgNADoW/KEOeAQub
R3rYOuw+ZAipQ7yl8m7eTnv14jcRzAPYtH1CW/AAAAAwEAAQAAAQA1pkJzJTaZ9bO+O77H
7DCXZsOf0L+VYGvtM31i0Xjjgp0SVDZWPQve4xDlnsON5nQVEhIOkPPZr4UTuImdU1Bqzi
+VNWVKCA+XXN2anbFTyPUMN1/6yhad2TUX3tmfRdIhwXqylbF+gFkT+TR56d0O/KpnU+QR
6GabIFhpJnz5Kfvt5cCfu/YxUAZr6q2Jn022LG9x6+3Togs/tT9FMBh62efBQNAaIcn/ZP
kX8UUjZ8LFW9rfEkIbadG6zhp+hOybPAfoDj1kmetaGZCm06F4TNrhI4Z2DSatcO5tQBEt
o3edC3nBJAE2UlTf2x8WlnS85aofCY0b5Wyw9VBoEO6xAAAAgQDEC20sW+Ljx7ZV1fzlFj
VR5CLZDojfsQ04zqamQwm2VbwybRY9xzgNWm56xnzjjgz+FQ9jZemxGK2RrXOShuEwKweF
6+DQIsTG6+vbmwTW/l3ox8Fe4BfwMapyXSTx7CbwSVk3EyrMeOYPElRd0LfPrqcn1a7AQD
dnOp2rQTZrEgAAAIEA4s/zWBthjgPVsx5w7+QqA6r4lbcQyLd48uacmB3Y91KADFD8BXDV
qZ8dtXFS8pykZdkHqVYqsARaKPSQrVGSTfr4K10aTwSOBbv9IpYuQK+XcyjkodJzYrWxHw
gjaqOULg1Ph96dDhILfYZoNBvkWbhMAhMPYvIO7RlO7SCqgg0AAACBANZCFOR7MaKYWyhu
M2XINoz7O9ChqN45BgzNUVarGm6Bo43fQbvkogZjTCHndPW5kafNC0IvbcVRdtoT+TkDJJ
qVrXPHjVosNivhFRNrGkcUUEwGDnVcfhEX/TQcVkvQVD4LcZldI1WnEKfxjuNZWLh0tU4M
K8qTrImDd2Ahj2/7AAAADnBhcnNlckBiYXN0aW9uAQIDBA==
-----END OPENSSH PRIVATE KEY-----
"""

    /// Den ska ge exakt rätt nyckelmaterial — och den ska ge exakt rätt
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

    /// ECDSA STÖDS — och ska ge rätt kurva, inte bara "någon ECDSA-nyckel".
    /// Landar den på fel kurva blir skalären fel längd och signaturen
    /// obrukbar, vilket för användaren ser ut som ett avvisat lösenord.
    func testAnEcdsaKeyParsesToTheCurveItWasGeneratedOn() throws {
        let auth = try OpenSSHPrivateKey.parse(Self.ecdsaPEM)
        guard case .ecdsa(let curve, let scalar) = auth else {
            return XCTFail("förväntade .ecdsa, fick \(auth)")
        }
        XCTAssertEqual(curve, .p256, "nyckeln genererades med ssh-keygen -t ecdsa (P256)")
        XCTAssertEqual(scalar.count, 32, "P256-skalären är alltid 32 byte efter utfyllnad")
        // Skalären ska gå att använda — inte bara ha rätt längd.
        XCTAssertNoThrow(try P256.Signing.PrivateKey(rawRepresentation: scalar))
    }

    /// RSA är den typ som faktiskt inte hanteras (medvetet, se ROADMAP.md
    /// "Uppskjutet med avsikt"). Felet ska säga VILKEN typ det var — utan
    /// det står användaren med en nyckel som inte fungerar och ingen
    /// ledtråd om varför.
    func testAnUnsupportedKeyTypeNamesTheTypeItFound() {
        XCTAssertThrowsError(try OpenSSHPrivateKey.parse(Self.rsaPEM)) { error in
            guard case .unsupportedKeyType(let type)? = error as? SSHKeyError else {
                return XCTFail("förväntade .unsupportedKeyType, fick \(error)")
            }
            XCTAssertTrue(type.contains("rsa"), "typen ska stå i felet: \(type)")
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
