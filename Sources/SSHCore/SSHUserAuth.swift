import Crypto
import Foundation
import NIOCore
import NIOSSH

/// Erbjuder autentiseringsuppgifter till servern. NIOSSH frågar oss om nästa
/// metod tills en lyckas eller alla är slut. När vi ger upp (returnerar nil)
/// anropas `onGiveUp` — NIOSSH stänger inte alltid anslutningen självt vid
/// misslyckad auth, så vi använder den signalen för att inte hänga.
final class SSHUserAuth: NIOSSHClientUserAuthenticationDelegate {
    private let username: String
    private let auth: SSHAuth
    private let onGiveUp: () -> Void
    private var offered = false

    init(username: String, auth: SSHAuth, onGiveUp: @escaping () -> Void) {
        self.username = username
        self.auth = auth
        self.onGiveUp = onGiveUp
    }

    private func giveUp(_ promise: EventLoopPromise<NIOSSHUserAuthenticationOffer?>) {
        onGiveUp()
        promise.succeed(nil)
    }

    func nextAuthenticationType(
        availableMethods: NIOSSHAvailableUserAuthenticationMethods,
        nextChallengePromise: EventLoopPromise<NIOSSHUserAuthenticationOffer?>
    ) {
        // Vi erbjuder varje uppgift en gång; misslyckas den ger vi upp.
        guard !offered else {
            giveUp(nextChallengePromise)
            return
        }
        offered = true

        switch auth {
        case .password(let pw):
            guard availableMethods.contains(.password) else {
                giveUp(nextChallengePromise)
                return
            }
            let offer = NIOSSHUserAuthenticationOffer(
                username: username, serviceName: "",
                offer: .password(.init(password: pw)))
            nextChallengePromise.succeed(offer)

        case .ed25519Seed(let seed):
            guard availableMethods.contains(.publicKey),
                  let key = try? Curve25519.Signing.PrivateKey(rawRepresentation: seed) else {
                giveUp(nextChallengePromise)
                return
            }
            let priv = NIOSSHPrivateKey(ed25519Key: key)
            let offer = NIOSSHUserAuthenticationOffer(
                username: username, serviceName: "",
                offer: .privateKey(.init(privateKey: priv)))
            nextChallengePromise.succeed(offer)

        case .ecdsa(let curve, let scalar):
            guard availableMethods.contains(.publicKey),
                  let priv = try? Self.niosshKey(curve: curve, scalar: scalar) else {
                giveUp(nextChallengePromise)
                return
            }
            let offer = NIOSSHUserAuthenticationOffer(
                username: username, serviceName: "",
                offer: .privateKey(.init(privateKey: priv)))
            nextChallengePromise.succeed(offer)

        case .secureEnclave(let stored):
            #if canImport(Darwin)
            guard availableMethods.contains(.publicKey),
                  let priv = try? SecureEnclaveKey.privateKey(from: stored) else {
                giveUp(nextChallengePromise)
                return
            }
            let offer = NIOSSHUserAuthenticationOffer(
                username: username, serviceName: "",
                offer: .privateKey(.init(privateKey: priv)))
            nextChallengePromise.succeed(offer)
            #else
            // Ingen Secure Enclave utanför Apple. Att tyst falla tillbaka
            // på något annat vore värre än att ge upp: användaren valde
            // hårdvarubunden nyckel av ett skäl.
            giveUp(nextChallengePromise)
            #endif

        case .certificate(let seed, let certificateLine):
            // Signerar med den RÅA privata nyckeln (samma som .ed25519Seed),
            // men erbjuder CERTIFIKATET som publik nyckel istället för den
            // bara nyckeln — swift-nio-ssh har förstklassigt certifikatstöd
            // (NIOSSHCertifiedPublicKey), inget eget behövs här. Servern
            // validerar CA-signaturen + giltighetsperiod + principal själv.
            //
            // Verifierad genom källkodsläsning + empiriskt (INTE bara antaget):
            // `NIOSSHUserAuthenticationOffer.Offer.PrivateKey(privateKey:certifiedKey:)`
            // signerar korrekt (bekräftat, "sign succeeded" i en temporär
            // instrumenterad körning). swift-nio-ssh SERVER-rollen kan dock
            // INTE ta emot certifikatbaserad publickey-auth — dess
            // `readUserAuthRequestMessage()` kollar det inkommande
            // algoritmnamnet mot `NIOSSHPublicKey.knownAlgorithms`, som bara
            // listar de fyra RÅA nyckeltyperna (`ssh-ed25519`/`ecdsa-*`),
            // aldrig `*-cert-v01@openssh.com`-varianterna — ett cert-erbjudande
            // blir tyst `.publicKey(.unknown)` och avvisas innan
            // `NIOSSHServerUserAuthenticationDelegate` ens nås. Påverkar INTE
            // Bastion i produktion (Bastion är alltid CLIENT, aldrig server —
            // en riktig sshd hanterar cert-auth fullt ut), men gör att den här
            // koden INTE kan täckas av ett genuint end-to-end-test mot
            // `LoopbackServer` (som använder swift-nio-ssh i serverrollen).
            // Se ROADMAP.md för detaljer.
            guard availableMethods.contains(.publicKey),
                  let key = try? Curve25519.Signing.PrivateKey(rawRepresentation: seed),
                  let publicKey = try? NIOSSHPublicKey(openSSHPublicKey: certificateLine),
                  let certifiedKey = NIOSSHCertifiedPublicKey(publicKey)
            else {
                giveUp(nextChallengePromise)
                return
            }
            let priv = NIOSSHPrivateKey(ed25519Key: key)
            let offer = NIOSSHUserAuthenticationOffer(
                username: username, serviceName: "",
                offer: .privateKey(.init(privateKey: priv, certifiedKey: certifiedKey)))
            nextChallengePromise.succeed(offer)
        }
    }

    private static func niosshKey(curve: ECDSACurve, scalar: Data) throws -> NIOSSHPrivateKey {
        switch curve {
        case .p256: return NIOSSHPrivateKey(p256Key: try P256.Signing.PrivateKey(rawRepresentation: scalar))
        case .p384: return NIOSSHPrivateKey(p384Key: try P384.Signing.PrivateKey(rawRepresentation: scalar))
        case .p521: return NIOSSHPrivateKey(p521Key: try P521.Signing.PrivateKey(rawRepresentation: scalar))
        }
    }
}
