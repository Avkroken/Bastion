import Crypto
import Foundation
import NIOCore
import NIOEmbedded
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

/// `TOFUHostKeyValidator` är beslutet: lär in okänd värd, acceptera
/// oförändrad, AVVISA ändrad. Den sista är hela MITM-skyddet, och det
/// var otestat — bara fingeravtrycksfunktionen bredvid hade tester.
///
/// `EmbeddedEventLoop` gör det testbart utan nätverk: promisen löses
/// synkront, så utfallet går att läsa direkt.
final class TOFUHostKeyValidatorTests: XCTestCase {

    private let ed25519Line =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIP0LeaRw74HwUKygzYCYT5ZroEZ0R/Zszy3kpAPzrT1B golden@bastion"
    private let otherLine =
        "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBHGafI4FczaW7rVd/oyg9oeW8xhptPF90WmC2gqWTVJ3h6/rk4F9Rx7sfMPxqLaJYNvSzO2XwQ2g3pWWR7GykR0= golden@bastion"

    /// Egen fil per test — annars läcker inlärda värdar mellan dem och
    /// ordningen börjar spela roll.
    private func temporaryStore() -> (KnownHosts, String) {
        let path = NSTemporaryDirectory() + "bastion-knownhosts-\(UUID().uuidString)"
        return (KnownHosts(path: path), path)
    }

    /// Promisen aldrig infriad — då har validatorn varken accepterat
    /// eller avvisat, vilket i skarp drift betyder att anslutningen hänger.
    private struct NeverCompleted: Error {}

    /// `whenComplete` tar en `@Sendable`-closure, och att mutera en fångad
    /// lokal `var` inifrån en sådan är ett kompileringsfel. En referensbox
    /// går bra: det är boxen som fångas, inte variabeln. `@unchecked` är
    /// korrekt här — allt sker på en enda tråd via `EmbeddedEventLoop`.
    private final class Box: @unchecked Sendable {
        var value: Result<Void, Error>?
    }

    /// `futureResult.wait()` går INTE att använda här: den kräver att
    /// anropet sker utanför event-loopen, och `EmbeddedEventLoop.inEventLoop`
    /// är alltid `true` — precondition-krasch i stället för ett testresultat.
    /// `whenComplete` + `run()` läser utfallet utan det antagandet.
    private func validate(
        _ validator: TOFUHostKeyValidator,
        _ line: String,
        on loop: EmbeddedEventLoop
    ) throws -> Result<Void, Error> {
        let key = try NIOSSHPublicKey(openSSHPublicKey: line)
        let promise = loop.makePromise(of: Void.self)
        let box = Box()
        promise.futureResult.whenComplete { box.value = $0 }
        validator.validateHostKey(hostKey: key, validationCompletePromise: promise)
        loop.run()
        guard let outcome = box.value else { throw NeverCompleted() }
        return outcome
    }

    /// En värd vi aldrig sett ska LÄRAS IN, inte avvisas. Annars vore
    /// första anslutningen omöjlig.
    func testUnknownHostIsLearnedAndAccepted() throws {
        let loop = EmbeddedEventLoop()
        let (store, path) = temporaryStore()
        defer { try? FileManager.default.removeItem(atPath: path) }

        var rejections: [HostKeyInfo] = []
        let validator = TOFUHostKeyValidator(
            host: "srv.example", port: 22, store: store, onReject: { rejections.append($0) }
        )

        guard case .success = try validate(validator, ed25519Line, on: loop) else {
            return XCTFail("en okänd värd ska lära in, inte avvisas")
        }
        XCTAssertTrue(rejections.isEmpty)
    }

    /// Samma nyckel igen ska accepteras tyst — det normala fallet vid
    /// varje anslutning efter den första.
    func testKnownUnchangedHostIsAccepted() throws {
        let loop = EmbeddedEventLoop()
        let (store, path) = temporaryStore()
        defer { try? FileManager.default.removeItem(atPath: path) }

        var rejections: [HostKeyInfo] = []
        let validator = TOFUHostKeyValidator(
            host: "srv.example", port: 22, store: store, onReject: { rejections.append($0) }
        )

        _ = try validate(validator, ed25519Line, on: loop)   // lär in
        guard case .success = try validate(validator, ed25519Line, on: loop) else {
            return XCTFail("oförändrad nyckel ska accepteras")
        }
        XCTAssertTrue(rejections.isEmpty)
    }

    /// Hela MITM-skyddet: en ANNAN nyckel för samma värd ska avvisas,
    /// promisen ska FAILA, och onReject ska anropas med fingeravtrycket
    /// — det senare är vad sessionen behöver för att stänga anslutningen
    /// i stället för att hänga.
    func testChangedHostKeyIsRejectedAndReported() throws {
        let loop = EmbeddedEventLoop()
        let (store, path) = temporaryStore()
        defer { try? FileManager.default.removeItem(atPath: path) }

        var rejections: [HostKeyInfo] = []
        let validator = TOFUHostKeyValidator(
            host: "srv.example", port: 22, store: store, onReject: { rejections.append($0) }
        )

        _ = try validate(validator, ed25519Line, on: loop)   // lär in den äkta

        guard case .failure(let error) = try validate(validator, otherLine, on: loop) else {
            return XCTFail("en ändrad värdnyckel MÅSTE avvisas — annars finns inget MITM-skydd")
        }
        guard let sshError = error as? SSHError,
              case .hostKeyRejected(let info) = sshError else {
            return XCTFail("fel typ av fel: \(error)")
        }
        XCTAssertEqual(info.keyType, "ecdsa-sha2-nistp256", "avtrycket ska gälla den NYA nyckeln")

        XCTAssertEqual(rejections.count, 1, "sessionen måste få veta, annars hänger anropet")
        XCTAssertEqual(rejections[0].sha256Fingerprint, info.sha256Fingerprint)
    }

    /// Samma värdnamn på en annan PORT är en annan värd. Slås de ihop
    /// avvisas en helt legitim andra server på samma maskin.
    func testSameHostOnADifferentPortIsTrackedSeparately() throws {
        let loop = EmbeddedEventLoop()
        let (store, path) = temporaryStore()
        defer { try? FileManager.default.removeItem(atPath: path) }

        let onPort22 = TOFUHostKeyValidator(
            host: "srv.example", port: 22, store: store, onReject: { _ in }
        )
        let onPort2222 = TOFUHostKeyValidator(
            host: "srv.example", port: 2222, store: store, onReject: { _ in }
        )

        _ = try validate(onPort22, ed25519Line, on: loop)
        guard case .success = try validate(onPort2222, otherLine, on: loop) else {
            return XCTFail("port ingår i identiteten — en annan port är en annan värd")
        }
    }
}
