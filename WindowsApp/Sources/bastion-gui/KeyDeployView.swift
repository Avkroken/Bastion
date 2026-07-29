import Foundation
import SSHCore
import SwiftCrossUI

/// Generera en ny SSH-nyckel, deploya den till fjärrsidans `authorized_keys`,
/// verifiera tyst att den faktiskt fungerar — och ENDAST om det lyckas,
/// erbjud (opt-in, aldrig automatiskt) att byta host-profilen till
/// nyckel-auth. Motsvarande "ta bort lösenordet" betyder här bara att sluta
/// FRÅGA efter lösenord för den här profilen (`.askPassword` -> `.keyFile`) —
/// LinuxApp har ingen Keychain och sparar aldrig själva lösenordet på disk
/// till att börja med, så det finns inget hemligt värde att radera separat.
/// Rör ALDRIG fjärrserverns egen autentiseringskonfiguration.
@MainActor
final class KeyDeployModel: ObservableObject {
    @Published var generatedKey: GeneratedKeyPair?
    @Published var deployed = false
    @Published var verified = false
    /// Sant EFTER en lyckad deploy mot en jump-hostad värd, där automatisk
    /// verifiering hoppades över (se `deployAndVerify()`). Skiljs från
    /// `verified` så vyn kan visa ett neutralt/varnande läge istället för
    /// rött "misslyckades" — deployen lyckades faktiskt, bara verifieringen
    /// kunde inte köras (cubic-fynd PR #179: annars visas en lyckad deploy
    /// som ett fel, och nyckeln blir ANVÄNDBAR på fjärrsidan men omöjlig att
    /// koppla till host-profilen via UI:t).
    @Published var deployedButUnverifiable = false
    @Published var statusMessage: String?
    @Published var importError: String?
    @Published var busy = false
    private let host: Host
    private let password: String?
    private let store: HostStore?
    private let comment: String

    init(host: Host, password: String?, store: HostStore? = nil) {
        self.host = host
        self.password = password
        self.store = store
        self.comment = "bastion-\(host.alias.isEmpty ? host.hostName : host.alias)"
    }

    /// Importerar en BEFINTLIG privat nyckel (klistrad OpenSSH PEM-text)
    /// istället för att generera en ny — samma efterföljande deploy+verify-
    /// flöde återanvänds rakt av, `generatedKey` sätts bara från en annan
    /// källa. Skiljer sig från HostEditViews importflöde: DEN vägen
    /// använder en redan existerande nyckel bara för AUTENTISERING (kopplar
    /// den till en host-profil), aldrig installerar den publika halvan på
    /// en fjärrserver — den här funktionen gör precis det.
    func importExisting(pem: String) {
        guard !busy else { return }
        importError = nil
        do {
            let auth = try OpenSSHPrivateKey.parse(pem)
            guard case .ed25519Seed(let seed) = auth else {
                importError = "Bara Ed25519-nycklar stöds."
                return
            }
            generatedKey = try KeyGenerator.fromExisting(seed: seed, comment: comment)
            deployed = false
            verified = false
            deployedButUnverifiable = false
            statusMessage = nil
        } catch SSHKeyError.encrypted {
            importError = "Lösenfras-skyddade nycklar stöds inte än."
        } catch {
            importError = "Kunde inte tolka nyckeln: \(error)"
        }
    }

    func generate() {
        // Busy-vakten hindrar att en pågående deployAndVerify() jobbar mot
        // en nyckel som hunnit bytas ut under tiden (CodeRabbit-fynd, PR #73)
        // — annars kunde ett SENT lyckat verifieringssvar för den GAMLA
        // nyckeln råka sätta `verified = true` efter att `generatedKey`
        // redan pekar på en NY, aldrig deployad/verifierad nyckel.
        guard !busy else { return }
        generatedKey = KeyGenerator.generateEd25519(comment: comment)
        deployed = false
        verified = false
        deployedButUnverifiable = false
        statusMessage = nil
        importError = nil
    }

    func deployAndVerify() async {
        guard let key = generatedKey else { return }
        busy = true
        deployed = false
        verified = false
        deployedButUnverifiable = false
        statusMessage = nil
        defer { busy = false }
        guard let plan = resolveConnectionPlan(for: host, password: password, store: store) else {
            statusMessage = "Kan inte autentisera värden (eller dess jump-host, om en är vald)."
            return
        }
        do {
            let chain = try await SSHConnectionChain.connect(target: host.target, targetAuth: plan.auth, jump: plan.jump)
            // Kedjan ska stängas oavsett om deployPublicKey lyckas eller
            // kastar — annars läcker en öppen SSH-anslutning vid fel
            // (CodeRabbit-fynd, PR #73: close() nåddes aldrig på felvägen).
            let deployResult: Result<Void, Error>
            do {
                try await chain.target.deployPublicKey(key.publicKeyLine, platform: host.platform)
                deployResult = .success(())
            } catch {
                deployResult = .failure(error)
            }
            await chain.close()
            try deployResult.get()
            deployed = true
        } catch {
            statusMessage = "Deploy misslyckades: \(error)"
            return
        }

        // `verifyKeyAuthWorks` stöder ännu inte en jump-parameter (kommer med
        // `Sources/SSHCore/KeyManagement.swift` när jump-host-fixarna i PR
        // #172 mergas) — hoppa över auto-verifiering för en jump-hostad värd
        // hellre än att felaktigt ansluta DIREKT och rapportera ett resultat
        // som inte speglar den riktiga anslutningsvägen.
        guard plan.jump == nil else {
            deployedButUnverifiable = true
            statusMessage = "Deployad (overifierad) — automatisk verifiering stöds ännu inte för värdar bakom en jump-host. Nyckeln fungerar sannolikt, men testa gärna manuellt innan du byter till den nedan."
            return
        }
        let ok = await SSHSession.verifyKeyAuthWorks(
            target: host.target, seed: key.seed, knownHosts: KnownHosts())
        // `generate()` kan inte köra medan `busy` är sant (se guarden ovan),
        // men kontrollen här är ändå den definitiva garantin: verifierings-
        // resultatet gäller bara om det fortfarande är SAMMA nyckel som
        // `generatedKey` pekar på.
        guard generatedKey?.publicKeyLine == key.publicKeyLine else {
            statusMessage = "Nyckeln ändrades under verifieringen. Kör deploy + verifiera igen."
            return
        }
        verified = ok
        statusMessage = ok
            ? "Nyckeln verifierad — fungerar."
            : "Verifiering misslyckades. Lösenordet är kvar, ingenting ändrat."
    }

    /// Skriver nyckeln till disk (`~/.bastion/keys/<host-id>_ed25519`, 0600)
    /// och returnerar en uppdaterad `Host` med `auth = .keyFile(path)`.
    /// Anropas bara efter att `verified` är sant — vyns checkbox styr NÄR,
    /// inte modellen (opt-in, matchar användarens uttryckliga krav).
    func saveKeyAndSwitchAuth() throws -> Host {
        // `deployedButUnverifiable` tillåts också (opt-in, användaren väljer
        // uttryckligen via `switchAuthAfterVerify`-togglen i vyn) — annars
        // vore en jump-hostad, faktiskt lyckad deploy permanent obrukbar via
        // UI:t tills #172:s jump-aware verifiering finns (cubic-fynd PR #179).
        guard let key = generatedKey, verified || deployedButUnverifiable else {
            throw SSHError.channelFailed("nyckeln är inte verifierad än")
        }
        let dir = ("~/.bastion/keys" as NSString).expandingTildeInPath
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        let path = dir + "/\(host.id.uuidString)_ed25519"
        let pem = try OpenSSHPrivateKey.export(seed: key.seed, comment: comment)
        try pem.write(toFile: path, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: path)

        var updated = host
        updated.auth = .keyFile(path)
        return updated
    }
}

struct KeyDeployView: View {
    @State private var model: KeyDeployModel
    @State private var switchAuthAfterVerify = false
    @State private var saveError: String?
    // "Klistra in befintlig" är ett HELT SKILT flöde från import på
    // värdredigeringssidan (HostEditView) — den använder bara en befintlig
    // nyckel för AUTENTISERING, aldrig installerar den publika halvan på en
    // fjärrserver. Den här knappen gör precis det.
    @State private var showPasteImport = false
    @State private var pastedKey = ""
    let onHostUpdated: (Host) -> Void

    init(host: Host, password: String?, store: HostStore? = nil, onHostUpdated: @escaping (Host) -> Void) {
        self._model = State(wrappedValue: KeyDeployModel(host: host, password: password, store: store))
        self.onHostUpdated = onHostUpdated
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("SSH-nyckel").font(.headline)
            Text("Genererar en ny Ed25519-nyckel, installerar den på fjärrservern och verifierar att den fungerar innan något ändras lokalt.")
                .foregroundColor(.gray)

            if showPasteImport {
                Text("Klistra in en befintlig OpenSSH-privatnyckel (samma format som ssh-keygen skriver). Den publika halvan installeras på fjärrservern, precis som en genererad nyckel.")
                    .foregroundColor(.gray)
                TextEditor(text: $pastedKey)
                    .frame(minHeight: 120)
                if let e = model.importError {
                    Text(e).foregroundColor(.red)
                }
                HStack {
                    Button("Avbryt") { showPasteImport = false; pastedKey = "" }
                    Button("Importera") {
                        model.importExisting(pem: pastedKey)
                        switchAuthAfterVerify = false
                        saveError = nil
                        if model.importError == nil { showPasteImport = false; pastedKey = "" }
                    }
                    .disabled(pastedKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }

            if let key = model.generatedKey {
                Text("Publik nyckel:").font(.subheadline)
                Text(key.publicKeyLine)
                    .font(.system(size: 11, design: .monospaced))
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            if let s = model.statusMessage {
                Text(s).foregroundColor(
                    model.verified ? .green : (model.deployedButUnverifiable ? .orange : .red))
            }
            if let e = saveError {
                Text(e).foregroundColor(.red)
            }

            HStack {
                Button(model.generatedKey == nil ? "Generera nyckel" : "Generera ny nyckel") {
                    model.generate()
                    switchAuthAfterVerify = false
                    saveError = nil
                    showPasteImport = false
                }
                .disabled(model.busy)
                if !showPasteImport {
                    Button("Klistra in befintlig nyckel istället") {
                        showPasteImport = true
                        pastedKey = ""
                    }
                    .disabled(model.busy)
                }
                if model.generatedKey != nil {
                    Button(model.busy ? "Arbetar…" : "Deploya + verifiera") {
                        Task { await model.deployAndVerify() }
                    }
                    .disabled(model.busy)
                }
            }

            if model.verified || model.deployedButUnverifiable {
                if model.deployedButUnverifiable {
                    Text("Overifierad — bekräfta bara om du själv har testat att nyckeln fungerar genom jump-hosten.")
                        .foregroundColor(.orange)
                }
                Toggle("Byt den här värden till nyckel-auth (fråga inte längre efter lösenord)", isOn: $switchAuthAfterVerify)
                Button("Bekräfta") { confirm() }
                    .disabled(!switchAuthAfterVerify)
            }
        }
        .padding()
    }

    private func confirm() {
        do {
            let updated = try model.saveKeyAndSwitchAuth()
            onHostUpdated(updated)
        } catch {
            saveError = "\(error)"
        }
    }
}
