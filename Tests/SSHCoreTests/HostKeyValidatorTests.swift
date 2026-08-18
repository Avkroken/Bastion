import Crypto
import Foundation
import NIOSSH
import XCTest
@testable import SSHCore

/// `fingerprint(of:)` är det som visas för användaren när en värdnyckel
/// ska godkännas eller när den ÄNDRATS. Stämmer den inte med vad
/// `ssh-keygen -lf` skriver går den inte att jämföra mot något — och då
/// är TOFU-skyddet en ritual utan innehåll.
///
/// Testvektorerna nedan är därför inte påhittade utan genererade med
/// riktiga `ssh-keygen` (OpenSSH 9.6) 2026-08-18, och det förväntade
/// värdet är exakt vad `ssh-keygen -lf` skrev ut för samma nyckel.
final class HostKeyFingerprintTests: XCTestCase {

    /// `ssh-keygen -t ed25519 -C golden@bastion`
    private static let ed25519Line =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIP0LeaRw74HwUKygzYCYT5ZroEZ0R/Zszy3kpAPzrT1B golden@bastion"
    /// Vad `ssh-keygen -lf` skrev för samma fil.
    private static let ed25519Fingerprint = "SHA256:H2t+bNQg9hzBJTSkk3T/CfOGQ/8d8vUg0XXVYiZ7eJw"

    /// `ssh-keygen -t ecdsa -C golden@bastion`
    private static let ecdsaLine =
        "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBHGafI4FczaW7rVd/oyg9oeW8xhptPF90WmC2gqWTVJ3h6/rk4F9Rx7sfMPxqLaJYNvSzO2XwQ2g3pWWR7GykR0= golden@bastion"
    private static let ecdsaFingerprint = "SHA256:kNgKkyaGFRvisz+FQMUIwzJl4j5vUV1Hs1eR6tstv1E"

    /// Kärnan: vårt fingeravtryck måste vara IDENTISKT med OpenSSH:s.
    /// Användaren jämför det mot vad servern eller en kollega uppger, och
    /// en avvikelse i formatet gör jämförelsen omöjlig utan att någonting
    /// ser trasigt ut.
    func testFingerprintMatchesSshKeygenForEd25519() throws {
        let key = try NIOSSHPublicKey(openSSHPublicKey: Self.ed25519Line)
        let info = fingerprint(of: key)
        XCTAssertEqual(info.sha256Fingerprint, Self.ed25519Fingerprint)
        XCTAssertEqual(info.keyType, "ssh-ed25519")
    }

    func testFingerprintMatchesSshKeygenForEcdsa() throws {
        let key = try NIOSSHPublicKey(openSSHPublicKey: Self.ecdsaLine)
        let info = fingerprint(of: key)
        XCTAssertEqual(info.sha256Fingerprint, Self.ecdsaFingerprint)
        XCTAssertEqual(info.keyType, "ecdsa-sha2-nistp256")
    }

    /// OpenSSH klipper bort utfyllnadstecknen. Ett fingeravtryck som
    /// slutar på `=` ser vid en snabb blick likadant ut men matchar inte
    /// vid en teckenjämförelse — och det är just teckenjämförelse
    /// användaren gör.
    func testFingerprintHasNoBase64Padding() throws {
        for line in [Self.ed25519Line, Self.ecdsaLine] {
            let info = fingerprint(of: try NIOSSHPublicKey(openSSHPublicKey: line))
            XCTAssertFalse(info.sha256Fingerprint.hasSuffix("="), info.sha256Fingerprint)
            XCTAssertTrue(info.sha256Fingerprint.hasPrefix("SHA256:"))
        }
    }

    /// Två olika nycklar måste ge två olika avtryck — annars skulle en
    /// bytt värdnyckel se oförändrad ut, vilket är exakt det TOFU ska
    /// fånga.
    func testDifferentKeysGiveDifferentFingerprints() throws {
        let a = fingerprint(of: try NIOSSHPublicKey(openSSHPublicKey: Self.ed25519Line))
        let b = fingerprint(of: try NIOSSHPublicKey(openSSHPublicKey: Self.ecdsaLine))
        XCTAssertNotEqual(a.sha256Fingerprint, b.sha256Fingerprint)
    }

    /// Samma nyckel ska ge samma avtryck varje gång. Trivialt men det är
    /// hela antagandet TOFU vilar på.
    func testFingerprintIsStableAcrossCalls() throws {
        let key = try NIOSSHPublicKey(openSSHPublicKey: Self.ed25519Line)
        XCTAssertEqual(fingerprint(of: key).sha256Fingerprint, fingerprint(of: key).sha256Fingerprint)
    }

    /// En nyckel som genererats i processen — inte från en textrad — ska
    /// ge ett avtryck i samma format. Fångar att kodvägen inte råkar
    /// bero på hur nyckeln kom in.
    func testFingerprintWorksForAFreshlyGeneratedKey() {
        let key = NIOSSHPrivateKey(ed25519Key: Curve25519.Signing.PrivateKey()).publicKey
        let info = fingerprint(of: key)
        XCTAssertTrue(info.sha256Fingerprint.hasPrefix("SHA256:"))
        // SHA-256 i base64 utan utfyllnad är alltid 43 tecken.
        XCTAssertEqual(info.sha256Fingerprint.count, "SHA256:".count + 43)
        XCTAssertEqual(info.keyType, "ssh-ed25519")
    }
}
