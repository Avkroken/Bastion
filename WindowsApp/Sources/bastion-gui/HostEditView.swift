import Foundation
import SSHCore
import SwiftCrossUI

/// Lägg till / ändra en värd. Motsvarar `App/HostEditView.swift`, men utan
/// Keychain-nyckelimport (se `AuthResolver.swift`) — bara sökväg till nyckelfil.
struct HostEditView: View {
    @State private var draft: Host
    @State private var portText: String
    @State private var tagsText: String
    @State private var startupCommandText: String
    @State private var macAddressText: String
    @State private var authKind: AuthKind
    @State private var keyPath: String
    @State private var certPath: String
    @State private var bitwardenItemIDText: String
    /// Övriga sparade värdar, för jump-host-väljaren nedan. Utesluter alltid
    /// `draft.id` själv (kan inte vara sin egen jump-host) — djupare
    /// cykeldetektering (A→B→A) görs inte här, bara den mest uppenbara.
    let allHosts: [Host]
    let onSave: (Host) -> Void
    let onCancel: () -> Void

    /// Picker-alternativ för jump-host-väljaren: "Ingen" plus varje giltig
    /// kandidat. Ett eget värdetyp istället för `Host?` direkt, eftersom
    /// SwiftCrossUIs `Picker` kräver `Equatable` och en textbeskrivning via
    /// `CustomStringConvertible`/interpolation.
    private enum JumpChoice: Equatable, CustomStringConvertible {
        case none
        case host(UUID, String)

        static func == (lhs: JumpChoice, rhs: JumpChoice) -> Bool {
            switch (lhs, rhs) {
            case (.none, .none): return true
            case (.host(let a, _), .host(let b, _)): return a == b
            default: return false
            }
        }

        var description: String {
            switch self {
            case .none: return "Ingen"
            case .host(_, let name): return name
            }
        }
    }

    /// Kandidater för jump host-väljaren: utesluter (1) `draft` själv, (2)
    /// `.askPassword`- och `.keychainKey`-värdar (går inte att autentisera
    /// automatiskt genom en jump-kedja — Linux/Windows har ingen Keychain
    /// och `resolveAuth` returnerar alltid `nil` för `.keychainKey` här, se
    /// `AuthResolver.swift`, så en sådan kandidat vore en deterministiskt
    /// obrukbar jump host), och (3) värdar som SJÄLVA har en jump-host satt —
    /// `SSHConnectionChain` stöder bara ETT hopp, så att välja en sådan
    /// kandidat skulle tyst hoppa över DESS jump-host och ansluta direkt
    /// till kandidaten istället (en kedja A→B→C skulle bara bli A→B,
    /// felaktigt utan varning). Detta gör cykler (A→B→A) strukturellt
    /// omöjliga också — en kandidat utan egen jump-host kan per definition
    /// inte peka tillbaka på något. Samma resonemang som App/HostEditView.swift.
    private var jumpCandidates: [Host] {
        allHosts.filter { candidate in
            guard candidate.id != draft.id else { return false }
            if case .askPassword = candidate.auth { return false }
            if case .keychainKey = candidate.auth { return false }
            return candidate.jumpHostID == nil
        }
    }

    private var jumpChoiceOptions: [JumpChoice] {
        [.none] + jumpCandidates.map { .host($0.id, $0.alias.isEmpty ? $0.hostName : $0.alias) }
    }

    private var jumpChoiceBinding: Binding<JumpChoice?> {
        Binding(
            get: {
                // `.some(.none)` — INTE bara `.none` — annars läser Swift
                // det som Optional.none (bare `nil`) i det här `JumpChoice?`-
                // sammanhanget, eftersom `JumpChoice` själv har ett `.none`-
                // fall (samma ambiguitet som `set` nedan redan undviker).
                // Med bare `.none` hittar SwiftCrossUIs Picker ingen matchning
                // i `jumpChoiceOptions` och rensar GTK-valet istället för att
                // visa "Ingen" som vald (cubic-fynd PR #179).
                guard let id = draft.jumpHostID,
                      let host = jumpCandidates.first(where: { $0.id == id })
                else { return .some(.none) }
                return .host(host.id, host.alias.isEmpty ? host.hostName : host.alias)
            },
            set: { choice in
                // `choice` är `JumpChoice?` — och `JumpChoice` har SJÄLV ett
                // `.none`-fall, så ett osyftat `case .none` här skulle bara
                // matcha Optional.none (widgeten utan val), inte vårt
                // explicita "Ingen"-val (`.some(.host)` är det enda fallet
                // som ska sätta ett jumpHostID).
                switch choice {
                case .some(.host(let id, _)): draft.jumpHostID = id
                default: draft.jumpHostID = nil
                }
            }
        )
    }

    enum AuthKind: Equatable, CustomStringConvertible {
        case agent, password, key, certificate
        /// Lösenordet hämtas ur en lokal Bitwarden CLI (`bw get password`)
        /// vid varje anslutning istället för att sparas — se `BitwardenClient`.
        /// Inget bifogat värde här (till skillnad från `importedElsewhere`) —
        /// själva item-id:t/namnet lever bara i `bitwardenItemIDText` (cubic-
        /// fynd: en bifogad payload här var död kod, aldrig läst, och kunde
        /// diverga från textfältet som `save()` faktiskt använder).
        case bitwarden
        /// Bevarar en `.keychainKey`-värd oförändrad — Linux/Windows saknar
        /// Keychain och kan varken läsa eller skriva sådana nycklar (se
        /// `AuthResolver.swift`), så vi rör inte kopplingen om användaren inte
        /// aktivt väljer bort den.
        case importedElsewhere(String)

        /// Jämför bara VILKET läge det är, inte det bifogade Keychain-id:t —
        /// samma resonemang som `JumpChoice.==` ovan.
        static func == (lhs: AuthKind, rhs: AuthKind) -> Bool {
            switch (lhs, rhs) {
            case (.agent, .agent), (.password, .password), (.key, .key),
                 (.certificate, .certificate), (.bitwarden, .bitwarden):
                return true
            case (.importedElsewhere, .importedElsewhere): return true
            default: return false
            }
        }

        var description: String {
            switch self {
            case .agent: return "Standardnyckel/agent"
            case .password: return "Fråga lösenord"
            case .key: return "Nyckelfil (sökväg)"
            case .certificate: return "OpenSSH-certifikat (nyckel + -cert.pub)"
            case .bitwarden: return "Bitwarden (bw CLI)"
            case .importedElsewhere: return "Importerad nyckel (endast iOS/macOS)"
            }
        }
    }

    /// Valbara lägen: de fem vanliga, plus — bara om värden redan har en —
    /// det bevarade Keychain-läget.
    private var pickerOptions: [AuthKind] {
        var options: [AuthKind] = [.agent, .password, .key, .certificate, .bitwarden]
        if case .keychainKey(let id) = draft.auth { options.append(.importedElsewhere(id)) }
        return options
    }

    /// `Picker` vill ha en `Binding<AuthKind?>`; vår state är alltid satt så
    /// vi översätter bara `set` genom `nil`.
    private var authKindBinding: Binding<AuthKind?> {
        Binding(get: { authKind }, set: { if let v = $0 { authKind = v } })
    }

    private var platformBinding: Binding<RemotePlatform?> {
        Binding(get: { draft.platform }, set: { if let v = $0 { draft.platform = v } })
    }

    init(host: Host, allHosts: [Host] = [], onSave: @escaping (Host) -> Void, onCancel: @escaping () -> Void) {
        self._draft = State(wrappedValue: host)
        self._portText = State(wrappedValue: String(host.port))
        self._tagsText = State(wrappedValue: host.tags.joined(separator: ", "))
        self._startupCommandText = State(wrappedValue: host.startupCommand ?? "")
        self._macAddressText = State(wrappedValue: host.macAddress ?? "")
        self.allHosts = allHosts
        self.onSave = onSave
        self.onCancel = onCancel
        self._bitwardenItemIDText = State(wrappedValue: {
            if case .bitwardenItem(let id) = host.auth { return id }
            return ""
        }())
        switch host.auth {
        case .agentDefault:
            self._authKind = State(wrappedValue: .agent)
            self._keyPath = State(wrappedValue: "")
            self._certPath = State(wrappedValue: "")
        case .askPassword:
            self._authKind = State(wrappedValue: .password)
            self._keyPath = State(wrappedValue: "")
            self._certPath = State(wrappedValue: "")
        case .keyFile(let p):
            self._authKind = State(wrappedValue: .key)
            self._keyPath = State(wrappedValue: p)
            self._certPath = State(wrappedValue: "")
        case .certificateFile(let keyPath, let certPath):
            self._authKind = State(wrappedValue: .certificate)
            self._keyPath = State(wrappedValue: keyPath)
            self._certPath = State(wrappedValue: certPath)
        case .keychainKey(let id):
            self._authKind = State(wrappedValue: .importedElsewhere(id))
            self._keyPath = State(wrappedValue: "")
            self._certPath = State(wrappedValue: "")
        case .bitwardenItem:
            self._authKind = State(wrappedValue: .bitwarden)
            self._keyPath = State(wrappedValue: "")
            self._certPath = State(wrappedValue: "")
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(draft.alias.isEmpty ? "Ny värd" : "Ändra värd").font(.headline)

            // ScrollView runt fälten (inte hela vyn) — antalet sektioner har
            // vuxit (jump host läggs till här) förbi vad en fast höjd alltid
            // rymmer, och Spara/Avbryt ska aldrig kunna hamna utanför
            // skärmen (samma mönster som Dashboard/Docker/PortForward-vyerna).
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    TextField("Alias (t.ex. Prod Web)", text: $draft.alias)
                    TextField("Värd (t.ex. 10.0.0.5)", text: $draft.hostName)
                    TextField("Användare", text: $draft.user)
                    TextField("Port", text: $portText)
                    TextField("Taggar (prod, homelab, …)", text: $tagsText)

                    Toggle("Favorit", isOn: $draft.isFavorite)
                    HostColorPicker(selection: $draft.colorTag)

                    Text("Autentisering").font(.subheadline)
                    Picker(of: pickerOptions, selection: authKindBinding)
                    // Grupperad — annars överskrider VStacken SwiftCrossUIs
                    // ViewBuilder-arity när ett tredje auth-detaljvillkor
                    // läggs till (samma gräns som tvingade fram `Group {}`
                    // i DashboardView.swift/S3BrowserView.swift).
                    Group {
                        if authKind == .key {
                            TextField("Sökväg till privatnyckel", text: $keyPath)
                        }
                        if authKind == .certificate {
                            TextField("Sökväg till privatnyckel", text: $keyPath)
                            TextField("Sökväg till certifikat (t.ex. nyckel-cert.pub)", text: $certPath)
                        }
                        if authKind == .bitwarden {
                            TextField("Bitwarden item-id eller namn", text: $bitwardenItemIDText)
                            Text("Kräver en giltig BW_SESSION i Bastions egen processmiljö — att låsa "
                                 + "upp valvet i en separat terminal räcker inte, sessionen förs inte "
                                 + "över automatiskt.")
                                .foregroundColor(.gray)
                        }
                    }

                    Text("Fjärrsystem").font(.subheadline)
                    Picker(of: RemotePlatform.allCases, selection: platformBinding)

                    TextField("Kör automatiskt vid anslutning (valfritt, t.ex. tmux attach)", text: $startupCommandText)

                    Text("Jump host").font(.subheadline)
                    Picker(of: jumpChoiceOptions, selection: jumpChoiceBinding)
                    if let jumpID = draft.jumpHostID, let jumpHost = jumpCandidates.first(where: { $0.id == jumpID }) {
                        Text("Ansluter genom \(jumpHost.alias.isEmpty ? jumpHost.hostName : jumpHost.alias) (ssh -J) innan den här värden nås.")
                            .foregroundColor(.gray)
                    }

                    Text("Wake-on-LAN").font(.subheadline)
                    TextField("MAC-adress (valfritt, t.ex. AA:BB:CC:DD:EE:FF)", text: $macAddressText)
                    if let message = macValidationMessage {
                        Text(message).foregroundColor(.orange)
                    }
                }
            }

            HStack {
                Button("Avbryt") { onCancel() }
                Spacer()
                Button("Spara") { save() }.disabled(!isValid)
            }
        }
        .padding()
    }

    /// `nil` om MAC-fältet (om ifyllt, trimmat) går att tolka — samma
    /// valideringsprincip som App/HostEditView.swift.
    private var macValidationMessage: String? {
        let trimmed = macAddressText.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return nil }
        do {
            _ = try WakeOnLan.parseMAC(trimmed)
            return nil
        } catch {
            return "Ogiltig MAC-adress."
        }
    }

    private var isValid: Bool {
        guard !draft.hostName.trimmingCharacters(in: .whitespaces).isEmpty,
              !draft.user.trimmingCharacters(in: .whitespaces).isEmpty,
              macValidationMessage == nil
        else { return false }
        switch authKind {
        case .key:
            return !keyPath.trimmingCharacters(in: .whitespaces).isEmpty
        case .certificate:
            return !keyPath.trimmingCharacters(in: .whitespaces).isEmpty
                && !certPath.trimmingCharacters(in: .whitespaces).isEmpty
        case .bitwarden:
            return !bitwardenItemIDText.trimmingCharacters(in: .whitespaces).isEmpty
        case .agent, .password, .importedElsewhere:
            return true
        }
    }

    private func save() {
        var host = draft
        host.port = Int(portText) ?? 22
        host.tags = tagsText.split(separator: ",")
            .map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        let trimmedStartup = startupCommandText.trimmingCharacters(in: .whitespacesAndNewlines)
        host.startupCommand = trimmedStartup.isEmpty ? nil : trimmedStartup
        let trimmedMac = macAddressText.trimmingCharacters(in: .whitespaces)
        host.macAddress = trimmedMac.isEmpty ? nil : trimmedMac
        switch authKind {
        case .agent: host.auth = .agentDefault
        case .password: host.auth = .askPassword
        case .key: host.auth = .keyFile(keyPath)
        case .certificate: host.auth = .certificateFile(keyPath: keyPath, certPath: certPath)
        case .bitwarden: host.auth = .bitwardenItem(bitwardenItemIDText.trimmingCharacters(in: .whitespaces))
        case .importedElsewhere(let id): host.auth = .keychainKey(id)
        }
        if host.alias.trimmingCharacters(in: .whitespaces).isEmpty { host.alias = host.hostName }
        onSave(host)
    }
}
