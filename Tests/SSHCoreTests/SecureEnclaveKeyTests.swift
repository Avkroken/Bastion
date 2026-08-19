import XCTest
@testable import SSHCore

/// Secure Enclave finns bara i Apples hårdvara, så merparten av modulen
/// går inte att köra i CI på Linux. Det som ÄR plattformsoberoende — att
/// tillgängligheten rapporteras ärligt, att felen går att skilja åt, och
/// att auth-fallet finns — testas här och körs överallt.
final class SecureEnclaveKeyTests: XCTestCase {

    /// Tillgängligheten måste FRÅGAS, inte antas. Koden kompilerar på
    /// varje plattform, men kretsen finns bara på vissa — och på Linux
    /// ska svaret vara ett tydligt nej i stället för ett fel längre fram.
    func testAvailabilityIsFalseWhereThereIsNoEnclave() {
        #if canImport(Darwin)
        // På Apple beror svaret på hårdvaran (CI-runners är ofta VM:ar
        // utan krets), så bara att anropet inte kraschar kan påstås här.
        _ = SecureEnclaveKey.isAvailable
        #else
        XCTAssertFalse(
            SecureEnclaveKey.isAvailable,
            "utan Darwin finns ingen Secure Enclave att rapportera"
        )
        #endif
    }

    /// De tre felen kräver olika saker av användaren: skaffa en nyare
    /// enhet, generera om nyckeln, eller försöka igen. En enda
    /// "det gick inte" hade dolt vilken.
    func testFailuresAreDistinguishable() {
        XCTAssertNotEqual(SecureEnclaveKey.Failure.unavailable, .corruptStoredKey)
        XCTAssertNotEqual(
            SecureEnclaveKey.Failure.generationFailed("a"),
            SecureEnclaveKey.Failure.generationFailed("b")
        )
        XCTAssertEqual(SecureEnclaveKey.Failure.unavailable, .unavailable)
    }

    /// Auth-fallet ska finnas i typen oavsett plattform — annars går det
    /// inte att spara en värd som använder Enclave på en maskin och
    /// synka den till en annan.
    func testAuthCaseExistsOnEveryPlatform() {
        let stored = Data([0x01, 0x02, 0x03])
        let auth = SSHAuth.secureEnclave(stored: stored)
        guard case .secureEnclave(let back) = auth else {
            return XCTFail("fallet ska gå att matcha")
        }
        XCTAssertEqual(back, stored)
    }

    /// Det sparade värdet är INTE den privata nyckeln utan en
    /// ogenomskinlig representation. Testet dokumenterar skillnaden så
    /// ingen frestas att behandla den som nyckelmaterial att skydda
    /// mindre noga — eller mer.
    #if canImport(Darwin)
    func testGenerationRequiresAnEnclaveAndSaysSoOtherwise() throws {
        guard !SecureEnclaveKey.isAvailable else {
            // På riktig hårdvara ska en nyckel gå att skapa och läsas
            // tillbaka till en användbar publik rad.
            let stored = try SecureEnclaveKey.generate()
            XCTAssertFalse(stored.isEmpty)
            let line = try SecureEnclaveKey.authorizedKeysLine(from: stored)
            XCTAssertTrue(
                line.hasPrefix("ecdsa-sha2-nistp256 "),
                "Enclave gör bara P-256, och raden ska säga det: \(line)"
            )
            XCTAssertTrue(line.hasSuffix(" bastion@secure-enclave"))
            return
        }
        // Utan krets ska felet vara .unavailable, inte något diffust.
        XCTAssertThrowsError(try SecureEnclaveKey.generate()) { error in
            XCTAssertEqual(error as? SecureEnclaveKey.Failure, .unavailable)
        }
    }
    #endif
}
