import Crypto
import Foundation
import NIOSSH

/// SSH-nyckel vars privata del ALDRIG lämnar Secure Enclave.
///
/// VISION under Säkerhet: *"Allt krypteras lokalt. Nycklar lämnar aldrig
/// enheten okrypterade. Face ID/Touch ID. Hardware-backed Secure Enclave
/// där möjligt."* Face ID finns sedan tidigare (`AppLockManager`); det
/// här är Enclave-delen.
///
/// # Vad "lämnar aldrig enheten" faktiskt betyder här
///
/// En vanlig nyckel i appen är en `Data` — den kan läsas, kopieras och
/// backas upp. En Enclave-nyckel kan inte det: den privata skalären
/// existerar bara inuti kretsen, och det enda som går att spara är en
/// ogenomskinlig `dataRepresentation` som är värdelös på varje annan
/// enhet. Signeringen sker i hårdvaran.
///
/// Skillnaden är alltså inte "bättre skyddad" utan "går inte att
/// exfiltrera" — ett stulet backup-arkiv ger ingenting.
///
/// # Varför P-256 och inte Ed25519
///
/// Secure Enclave gör bara NIST P-256. Resten av appen föredrar Ed25519,
/// och det är fortfarande förvalet — det här är ett alternativ för den
/// som vill ha hårdvarubindning, inte en ersättning.
///
/// # Varför det här inte var blockerat
///
/// ROADMAP noterade att PKCS11/YubiKey/passkeys inte går att koppla in i
/// swift-nio-ssh eftersom `NIOSSHPrivateKey.backingKey` är ett internt
/// enum med fasta fall. Secure Enclave är undantaget: ett av de fasta
/// fallen ÄR `.secureEnclaveP256`, och det finns en publik initierare
/// `NIOSSHPrivateKey(secureEnclaveP256Key:)` (verifierad i 0.15.0,
/// `NIOSSHPrivateKey.swift` rad 56, bakom `#if canImport(Darwin)`).
/// Blockeringen gällde externa tokens, inte hårdvaran i enheten.
public enum SecureEnclaveKey {

    /// Varför en Enclave-nyckel inte gick att använda.
    public enum Failure: Error, Equatable {
        /// Enheten saknar Secure Enclave (simulator, äldre Mac, Linux).
        case unavailable
        /// Den sparade representationen gick inte att läsa tillbaka.
        case corruptStoredKey
        /// Kretsen vägrade skapa nyckeln.
        case generationFailed(String)
    }

    /// Finns Secure Enclave på den här enheten?
    ///
    /// Måste frågas — koden kompilerar på varje Apple-plattform men
    /// simulatorn och äldre Mac-modeller saknar kretsen, och där ska
    /// användaren få veta det i stället för ett kryptiskt fel vid
    /// anslutning.
    public static var isAvailable: Bool {
        #if canImport(Darwin)
        return SecureEnclave.isAvailable
        #else
        return false
        #endif
    }
}

#if canImport(Darwin)

extension SecureEnclaveKey {

    /// Skapar en ny nyckel i kretsen.
    ///
    /// Det som kommer tillbaka är representationen att SPARA — inte
    /// nyckeln. Den går inte att få ut.
    public static func generate() throws -> Data {
        guard isAvailable else { throw Failure.unavailable }
        do {
            let key = try SecureEnclave.P256.Signing.PrivateKey()
            return key.dataRepresentation
        } catch {
            throw Failure.generationFailed(String(describing: error))
        }
    }

    /// Läser tillbaka en sparad nyckel.
    private static func load(_ stored: Data) throws -> SecureEnclave.P256.Signing.PrivateKey {
        guard isAvailable else { throw Failure.unavailable }
        do {
            return try SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: stored)
        } catch {
            throw Failure.corruptStoredKey
        }
    }

    /// Nyckeln i den form swift-nio-ssh vill ha den.
    public static func privateKey(from stored: Data) throws -> NIOSSHPrivateKey {
        NIOSSHPrivateKey(secureEnclaveP256Key: try load(stored))
    }

    /// Raden att klistra in i `~/.ssh/authorized_keys` på servern.
    ///
    /// Utan den här är nyckeln oanvändbar: servern måste känna till den
    /// publika delen, och den finns inte någon annanstans att hämta.
    /// Kommentaren är default `bastion@secure-enclave` så raden går att
    /// känna igen bland andra nycklar.
    public static func authorizedKeysLine(
        from stored: Data,
        comment: String = "bastion@secure-enclave"
    ) throws -> String {
        let publicKey = try privateKey(from: stored).publicKey
        var line = String(openSSHPublicKey: publicKey)
        if !comment.isEmpty {
            line += " " + comment
        }
        return line
    }
}

#endif
