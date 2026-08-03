import Foundation
import SSHCore

/// `Foundation.Host` finns även på Linux och krockar annars med `SSHCore.Host`
/// — samma fix som `App/Platform.swift` gör för iOS/macOS.
typealias Host = SSHCore.Host

/// Bygger `SSHAuth` för en värd. Samma logik som `App/AuthResolver.swift`,
/// men utan Keychain — Linux/Windows saknar en motsvarighet ännu.
func resolveAuth(for host: Host, password: String?) -> SSHAuth? {
    switch host.auth {
    case .askPassword:
        return password.map { SSHAuth.password($0) }
    case .keyFile(let path):
        return try? OpenSSHPrivateKey.load(path: path)
    case .agentDefault:
        let def = ("~/.ssh/id_ed25519" as NSString).expandingTildeInPath
        return try? OpenSSHPrivateKey.load(path: def)
    case .keychainKey:
        // Importerade nycklar lagras i Apples Keychain (se App/Keychain.swift) —
        // inget motsvarande säkert nyckelvalv finns här än. Använd Nyckelfil
        // (sökväg) eller standardnyckel/agent på Linux/Windows tills vidare.
        return nil
    case .certificateFile(let keyPath, let certPath):
        return try? OpenSSHPrivateKey.loadCertificate(keyPath: keyPath, certPath: certPath)
    case .bitwardenItem(let itemID):
        do {
            return .password(try BitwardenClient.fetchPassword(itemID: itemID))
        } catch {
            // Loggas (inte bara tystad via `try?`) så orsaken (saknad `bw`,
            // utgången BW_SESSION, fel item-id) syns i stället för ett
            // generiskt anslutningsfel — cubic-fynd på PR #185.
            FileHandle.standardError.write(Data("Bitwarden-hämtning för \"\(itemID)\" misslyckades: \(error)\n".utf8))
            return nil
        }
    }
}

/// Löser upp anslutningsplanen (mål-auth + ev. jump-host) för en värd —
/// samma anslutningsplansmönster som `App/SessionView.swift`s `plan`
/// (PR #172, som lägger detta som en delad `resolveConnectionPlan`-
/// funktion i `App/AuthResolver.swift`, flyttar referensen dit igen).
/// `store` är `nil` på anropsplatser som inte har en delad `HostStore`
/// tillgänglig; en host UTAN `jumpHostID` ansluter då fortfarande direkt,
/// men en host MED `jumpHostID` nekas anslutning istället för att tyst
/// hoppa förbi jump-hosten (fail-closed, inte en tyst degradering).
func resolveConnectionPlan(
    for host: Host, password: String?, store: HostStore?
) -> (auth: SSHAuth, jump: (target: SSHTarget, auth: SSHAuth)?)? {
    let jumpHost = host.jumpHostID.flatMap { store?.get($0) }
    let jumpAuth = jumpHost.flatMap { resolveAuth(for: $0, password: nil) }
    let result = ConnectionPlanning.plan(
        targetAuth: resolveAuth(for: host, password: password),
        jumpHostID: host.jumpHostID, jumpHost: jumpHost, jumpAuth: jumpAuth)
    return try? result.get()
}
