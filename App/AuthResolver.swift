#if canImport(SwiftUI)
import Foundation
import SSHCore

/// Bygger `SSHAuth` för en värd. Delas av dashboard och terminal så bägge
/// autentiserar likadant.
func resolveAuth(for host: Host, password: String?) -> SSHAuth? {
    switch host.auth {
    case .askPassword:
        return password.map { SSHAuth.password($0) }
    case .keyFile(let path):
        return try? OpenSSHPrivateKey.load(path: path)
    case .agentDefault:
        let def = ("~/.ssh/id_ed25519" as NSString).expandingTildeInPath
        return try? OpenSSHPrivateKey.load(path: def)
    case .keychainKey(let id):
        guard let pem = Keychain.get(id) else { return nil }
        return try? OpenSSHPrivateKey.parse(pem)
    case .certificateFile(let keyPath, let certPath):
        return try? OpenSSHPrivateKey.loadCertificate(keyPath: keyPath, certPath: certPath)
    case .bitwardenItem:
        // BÅDA App/-plattformarna saknar en säker väg att köra `bw` här:
        // iOS har inget `Foundation.Process` alls, och macOS-målets App
        // Sandbox (`com.apple.security.app-sandbox: true`, se
        // `App/project.yml`) dödar processen med ett okatchbart SIGTRAP så
        // fort `Process.run()` försöker starta en extern, osignerad binär
        // som `bw` — empiriskt verifierat på riktig macOS-hårdvara
        // (2026-07-20). `#if !os(iOS)` här hade fortsatt låtit detta nås på
        // macOS för en värd synkad från LinuxApp, där `bw` faktiskt fungerar
        // (tidigare telemetryfynd CRITICAL +
        // cubic P1 på PR #185: UI-filtret i HostEditView hindrar bara NYA
        // val, inte redan synkade värdar som väljs för ANSLUTNING). Kraschen
        // är inte en Swift-error ett `do/catch` kan fånga, så det enda
        // korrekta är att aldrig försöka — strukturellt omöjligt, inte bara
        // UI-dolt.
        return nil
    }
}

/// Löser upp target-auth OCH, om `host.jumpHostID` är satt, jump-endpointen
/// — enda stället som känner till den regeln, delad av alla anropsplatser
/// som kan koppla upp en `SSHConnectionChain`. Returnerar `nil` om target
/// INTE kan autentiseras, eller om en konfigurerad jump-host är satt men
/// saknas i `store`/inte kan autentiseras/SJÄLV har en jump-host satt: en
/// jump-host ska ALDRIG hoppas över tyst eller anslutas genom en ofullständig
/// kedja (`SSHConnectionChain` stöder bara ETT hopp — en B med egen
/// `jumpHostID` skulle annars tyst ansluta B→target och ignorera B:s C, t.ex.
/// om B redigerats efter att A redan pekade på B). Det vore en
/// säkerhetsregression för den som medvetet satt upp en jump-host (se
/// `SessionView.plan`, samma kontrakt).
///
/// Själva beslutslogiken (`ConnectionPlanning.plan`) är utbruten till
/// SSHCore och testad i `SSHCoreTests` — den delen är ren (tar redan
/// upplösta `SSHAuth?`-värden) och behöver inte Keychain/Bitwarden. Den HÄR
/// funktionen kollapsar fortfarande alla fyra felfall till `nil` (cubic-fynd
/// på PR #172 om att detta döljer VILKET fel det var) — att visa orsaken i
/// UI kräver att ändra returtypen i ~16 anropsplatser, ett scopat
/// uppföljningsarbete.
func resolveConnectionPlan(
    for host: Host, password: String?, store: HostStore?
) -> (auth: SSHAuth, jump: (target: SSHTarget, auth: SSHAuth)?)? {
    let targetAuth = resolveAuth(for: host, password: password)
    let jumpHost = host.jumpHostID.flatMap { store?.get($0) }
    // `jumpAuth` löses bara upp om planen ens KAN lyckas — annars skulle en
    // redan dömd anslutning (target-auth misslyckad, jump-host saknas, eller
    // nästlad) ändå trigga onödig Keychain-/privatnyckelåtkomst för
    // jump-hosten (t.ex. ett Touch ID-prompt) för en anslutning som aldrig
    // blir av (cubic P3 på PR #172).
    guard targetAuth != nil, let jumpHost, jumpHost.jumpHostID == nil else {
        let result = ConnectionPlanning.plan(
            targetAuth: targetAuth, jumpHostID: host.jumpHostID, jumpHost: jumpHost, jumpAuth: nil)
        return try? result.get()
    }
    let jumpAuth = resolveAuth(for: jumpHost, password: nil)
    let result = ConnectionPlanning.plan(
        targetAuth: targetAuth, jumpHostID: host.jumpHostID, jumpHost: jumpHost, jumpAuth: jumpAuth)
    return try? result.get()
}
#endif
