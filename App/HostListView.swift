#if canImport(SwiftUI)
import SwiftUI
import SSHCore

/// Delar host-databasen till vyerna och håller den observerbar.
@MainActor
final class HostListModel: ObservableObject {
    // Inte `private` — HostListView behöver dela SAMMA instans med
    // MultiSessionView/HostDetailView (se HostDetailView.swift, CodeRabbit-
    // fynd #126), inte låta dem skapa sina egna.
    let store = HostStore()
    let snippets = SnippetStore()   // synkas i samma rundtur som värdarna
    @Published var hosts: [Host] = []

    init() { reload() }
    func reload() { hosts = store.all() }
    func save(_ host: Host) { store.upsert(host); reload() }
    func delete(_ host: Host) {
        if case .keychainKey(let id) = host.auth { Keychain.delete(id) }
        store.delete(host.id)
        reload()
    }
    func toggleFavorite(_ host: Host) {
        var h = host
        h.isFavorite.toggle()
        save(h)
    }

    @discardableResult
    func importConfig(_ text: String) -> Int {
        let n = store.importSSHConfig(text).count
        reload()
        return n
    }

    /// E2E-krypterad synkrunda mot den konfigurerade transporten (mapp eller ett kontointegrerat moln).
    func syncNow() async -> String {
        guard UserDefaults.standard.bool(forKey: SyncKeys.enabled) else { return "Sync är avstängd." }
        guard let pass = Keychain.get(SyncKeys.passphraseKey), !pass.isEmpty else {
            return "Ingen lösenfras angiven."
        }
        let transport = UserDefaults.standard.string(forKey: SyncKeys.transport) ?? "folder"

        func requireLogin(_ config: OAuthProviderConfig) -> String? {
            OAuthAccountManager.shared.isLoggedIn(config) ? nil : "Inte inloggad på \(config.displayName)."
        }

        let provider: SyncProvider
        // Security-scoped mapp-URL som måste hållas öppen HELA vägen genom
        // store.sync(); stängs i defer efter synkrundan.
        var scopedFolder: URL?
        switch transport {
        case "dropbox":
            if let err = requireLogin(OAuthProviders.dropbox) { return err }
            provider = DropboxSyncProvider(passphrase: pass)
        case "googledrive":
            if let err = requireLogin(OAuthProviders.googleDrive) { return err }
            provider = GoogleDriveSyncProvider(passphrase: pass)
        case "onedrive":
            if let err = requireLogin(OAuthProviders.oneDrive) { return err }
            provider = OneDriveSyncProvider(passphrase: pass)
        default:
            guard let url = SyncFolder.resolve() else { return "Ingen synkmapp vald." }
            guard url.startAccessingSecurityScopedResource() else {
                return "Kommer inte åt synkmappen — välj den igen i Sync-inställningarna."
            }
            scopedFolder = url
            let file = url.appendingPathComponent("bastion-sync.enc").path
            provider = EncryptedFolderSyncProvider(path: file, passphrase: pass)
        }
        defer { scopedFolder?.stopAccessingSecurityScopedResource() }
        do {
            try store.sync(with: provider, snippets: snippets)
            reload()
            return "Synkat."
        } catch {
            return "Sync misslyckades: \(error.localizedDescription)"
        }
    }

    /// Grupperad efter tagg; otaggade hamnar under "Övriga". Favoriter får en
    /// egen sektion allra först och plockas ur sin vanliga taggsektion (annars
    /// skulle samma Host-id förekomma två gånger i samma List/ForEach, vilket
    /// SwiftUI inte diffar tillförlitligt).
    var groups: [(tag: String, hosts: [Host])] {
        var byTag: [String: [Host]] = [:]
        for h in hosts where !h.isFavorite {
            let tags = h.tags.isEmpty ? ["Övriga"] : h.tags
            for t in tags { byTag[t, default: []].append(h) }
        }
        let tagged = byTag.keys.sorted { $0.lowercased() < $1.lowercased() }
            .map { (tag: $0, hosts: byTag[$0]!.sorted { $0.alias.lowercased() < $1.alias.lowercased() }) }
        let favorites = hosts.filter(\.isFavorite).sorted { $0.alias.lowercased() < $1.alias.lowercased() }
        guard !favorites.isEmpty else { return tagged }
        return [(tag: "★ Favoriter", hosts: favorites)] + tagged
    }
}

/// Anslutningsbegäran: vald värd + eventuellt inmatat lösenord.
struct ConnectRequest: Identifiable {
    let id = UUID()
    let host: Host
    let password: String?
    /// Kommando att köra direkt i shellen (t.ex. `docker exec …`).
    var initialCommand: String? = nil

    /// Kopia som öppnar en shell med ett startkommando.
    func running(_ command: String) -> ConnectRequest {
        ConnectRequest(host: host, password: password, initialCommand: command)
    }
}

struct HostListView: View {
    @StateObject private var model = HostListModel()
    @StateObject private var sessionManager = SessionManager()
    @State private var editing: Host?
    @State private var showSessions = false
    @State private var passwordFor: Host?
    @State private var passwordInput = ""
    @State private var autofillUsername = ""
    @State private var showSettings = false
    @State private var showImport = false
    @State private var showAppLock = false
    @State private var showWireGuard = false
    @State private var showTailscale = false
    @State private var pendingHostFromDiscovery: Host?
    @State private var showS3 = false
    @State private var showTerminalTheme = false
    @State private var showDebugLog = false
    @State private var showFeatureSettings = false
    @State private var showQuickConnect = false
    @State private var pendingQuickConnectRequest: ConnectRequest?
    @State private var showTelnetConnect = false
    @State private var pendingTelnetTarget: TelnetTarget?
    @State private var telnetTarget: TelnetTarget?
    #if os(macOS)
    // Seriell/USB — bara macOS, se filkommentaren i SerialConnectView.swift.
    @State private var showSerialConnect = false
    @State private var pendingSerialConfig: SerialConfig?
    @State private var serialConfig: SerialConfig?
    #endif
    @State private var searchText = ""
    @State private var wakeMessage: String?

    /// `model.groups` filtrerat på sökfältet (alias/hostname/user/taggar,
    /// case-insensitive); tomma sektioner (ingen träff i gruppen) faller bort.
    private var filteredGroups: [(tag: String, hosts: [Host])] {
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return model.groups }
        let needle = trimmed.lowercased()
        return model.groups.compactMap { group in
            let hosts = group.hosts.filter { host in
                host.alias.lowercased().contains(needle)
                    || host.hostName.lowercased().contains(needle)
                    || host.user.lowercased().contains(needle)
                    || host.tags.contains { $0.lowercased().contains(needle) }
            }
            return hosts.isEmpty ? nil : (group.tag, hosts)
        }
    }

    var body: some View {
        NavigationStack {
            Group {
                if model.hosts.isEmpty {
                    ContentUnavailableView("Inga värdar än", systemImage: "server.rack",
                                           description: Text("Lägg till din första server med +."))
                } else {
                    hostList
                }
            }
            .navigationTitle("Värdar")
            .searchable(text: $searchText, prompt: "Sök värd, användare eller tagg")
            .task { _ = await model.syncNow() }
            .toolbar {
                ToolbarItem(placement: .navigation) {
                    Menu {
                        Button { showSettings = true } label: { Label("Sync-inställningar", systemImage: "arrow.triangle.2.circlepath") }
                        Button { showImport = true } label: { Label("Importera ssh-config", systemImage: "square.and.arrow.down") }
                        Button { showAppLock = true } label: { Label("App-lås", systemImage: "faceid") }
                        Button { showWireGuard = true } label: { Label("WireGuard-profiler", systemImage: "network.badge.shield.half.filled") }
                        Button { showTailscale = true } label: { Label("Tailscale-värdar", systemImage: "point.3.filled.connected.trianglepath.dotted") }
                        Button { showS3 = true } label: { Label("S3-lagring", systemImage: "externaldrive.badge.icloud") }
                        Button { showTerminalTheme = true } label: { Label("Terminaltema", systemImage: "paintpalette") }
                        Button { showFeatureSettings = true } label: { Label("Funktioner", systemImage: "switch.2") }
                        Button { showQuickConnect = true } label: { Label("Snabbanslutning", systemImage: "bolt.horizontal") }
                        Button { showTelnetConnect = true } label: { Label("Telnet", systemImage: "terminal") }
                        #if os(macOS)
                        Button { showSerialConnect = true } label: { Label("Seriell/USB", systemImage: "cable.connector") }
                        #endif
                        Button { showDebugLog = true } label: { Label("Debug-logg", systemImage: "ladybug") }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                }
                ToolbarItem(placement: .primaryAction) {
                    Button { editing = Self.newHost } label: {
                        Image(systemName: "plus")
                    }
                }
            }
            .sheet(item: $editing) { host in
                HostEditView(host: host, allHosts: model.hosts) { model.save($0); editing = nil }
            }
            .sheet(isPresented: $showSettings) {
                SyncSettingsView(syncNow: { await model.syncNow() })
            }
            .sheet(isPresented: $showImport) {
                ImportConfigView(onImport: { model.importConfig($0) })
            }
            .sheet(isPresented: $showAppLock) {
                AppLockSettingsView()
            }
            .sheet(isPresented: $showWireGuard) {
                WireGuardProfileListView()
            }
            .sheet(isPresented: $showTailscale, onDismiss: {
                // Sätts INTE direkt i onAddHost — den sheeten är fortfarande
                // på väg att stängas då, och att sätta $editing samtidigt
                // krockar med SwiftUIs single-sheet-hantering (CodeRabbit-
                // fynd, #115). Vänta tills Tailscale-sheeten faktiskt stängt.
                if let pending = pendingHostFromDiscovery {
                    pendingHostFromDiscovery = nil
                    editing = pending
                }
            }) {
                TailscaleDiscoveryView(
                    hosts: model.hosts,
                    store: model.store,
                    onAddHost: { alias, hostName in
                        pendingHostFromDiscovery = Host(alias: alias, hostName: hostName, user: "")
                    }
                )
            }
            .sheet(isPresented: $showS3) {
                S3ConnectionListView()
            }
            .sheet(isPresented: $showTerminalTheme) {
                TerminalThemeSettingsView()
            }
            .sheet(isPresented: $showFeatureSettings) {
                FeatureSettingsView()
            }
            .sheet(isPresented: $showDebugLog) {
                DebugLogView()
            }
            .sheet(isPresented: $showQuickConnect, onDismiss: {
                // Samma mönster som Tailscale-upptäckten ovan: öppna sessionen
                // FÖRST efter att QuickConnectView-sheeten faktiskt stängt —
                // att sätta showSessions medan den fortfarande stänger krockar
                // med SwiftUIs single-sheet-hantering (cubic-fynd, PR #173).
                if let request = pendingQuickConnectRequest {
                    pendingQuickConnectRequest = nil
                    sessionManager.open(request)
                    showSessions = true
                }
            }) {
                QuickConnectView { request in
                    pendingQuickConnectRequest = request
                }
            }
            .sheet(isPresented: $showTelnetConnect, onDismiss: {
                // Samma mönster som Tailscale-upptäckten ovan: sätt målet FÖRST
                // efter att TelnetConnectView-sheeten faktiskt stängt — annars
                // krockar det med SwiftUIs single-sheet-hantering (samma
                // sentry/cubic-fynd som Quick Connect råkade ut för, PR #173).
                if let pending = pendingTelnetTarget {
                    pendingTelnetTarget = nil
                    telnetTarget = pending
                }
            }) {
                TelnetConnectView { target in
                    pendingTelnetTarget = target
                }
            }
            #if os(macOS)
            .sheet(isPresented: $showSerialConnect, onDismiss: {
                // Samma mönster som Telnet ovan.
                if let pending = pendingSerialConfig {
                    pendingSerialConfig = nil
                    serialConfig = pending
                }
            }) {
                SerialConnectView { config in
                    pendingSerialConfig = config
                }
            }
            #endif
            .cover(isPresented: $showSessions) {
                MultiSessionView(manager: sessionManager, store: model.store)
            }
            #if canImport(SwiftTerm) && (os(iOS) || os(macOS))
            .cover(item: $telnetTarget) { target in
                TelnetSessionView(target: target)
            }
            #endif
            #if canImport(SwiftTerm) && os(macOS)
            .cover(item: $serialConfig) { config in
                SerialSessionView(config: config)
            }
            #endif
            // Sista fliken stängd -> tillbaka till värdlistan automatiskt,
            // inte kvar på en tom flikväxlare.
            .onChange(of: sessionManager.sessions.isEmpty) { isEmpty in
                if isEmpty { showSessions = false }
            }
            .alert("Lösenord", isPresented: .constant(passwordFor != nil)) {
                // Ett förifyllt användarnamnsfält parat med lösenordsfältet
                // nedan — utan det kan inte AutoFill matcha rätt sparade
                // konto (visas annars som en ospecificerad lista, inte
                // "Lösenord för <user>" som i Termius). MÅSTE vara redigerbart
                // — ett `.disabled`-fält kan inte bli first responder och
                // exkluderas därför helt ur AutoFill-sammanhanget (verifierat
                // resonemang, inte antaget). Anslutningen använder alltid
                // `passwordFor.user`, aldrig detta fälts värde, så en
                // eventuell redigering här är ofarlig — den påverkar bara
                // vilket AutoFill-förslag som visas.
                TextField("Användare", text: $autofillUsername)
                    .autofillUsername()
                SecureField("Lösenord", text: $passwordInput)
                    .autofillPassword()
                Button("Anslut") {
                    if let h = passwordFor {
                        sessionManager.open(
                            ConnectRequest(host: h, password: passwordInput, initialCommand: h.startupCommand))
                        showSessions = true
                    }
                    passwordFor = nil; passwordInput = ""
                }
                Button("Avbryt", role: .cancel) { passwordFor = nil; passwordInput = "" }
            }
            .alert("Wake-on-LAN", isPresented: .constant(wakeMessage != nil), presenting: wakeMessage) { _ in
                Button("OK") { wakeMessage = nil }
            } message: { message in
                Text(message)
            }
        }
    }

    /// Skickar ett magic packet till `host.macAddress` — fel visas i en alert
    /// istället för att sväljas tyst, men blockerar aldrig anslutningsflödet
    /// (WoL är best-effort: paketet kan skickas iväg utan att målet faktiskt
    /// vaknar, t.ex. om det redan är på eller inte lyssnar efter WoL).
    private func wake(_ host: Host) {
        guard let mac = host.macAddress else { return }
        Task {
            do {
                try await WakeOnLan.send(mac: mac)
                wakeMessage = "Skickade väckningssignal till \(host.alias.isEmpty ? host.hostName : host.alias)."
            } catch {
                wakeMessage = "Kunde inte skicka väckningssignal: \(error)"
            }
        }
    }

    private var hostList: some View {
        List {
            ForEach(filteredGroups, id: \.tag) { group in
                Section(group.tag) {
                    ForEach(group.hosts) { host in
                        Button { start(host) } label: { HostRow(host: host) }
                            .swipeActions(edge: .trailing) {
                                Button(role: .destructive) { model.delete(host) } label: {
                                    Label("Ta bort", systemImage: "trash")
                                }
                                Button { editing = host } label: {
                                    Label("Ändra", systemImage: "pencil")
                                }.tint(.blue)
                            }
                            .swipeActions(edge: .leading) {
                                Button { model.toggleFavorite(host) } label: {
                                    Label(host.isFavorite ? "Ta bort favorit" : "Favorit",
                                          systemImage: host.isFavorite ? "star.slash" : "star")
                                }.tint(.yellow)
                                if host.macAddress != nil {
                                    Button { wake(host) } label: {
                                        Label("Väck", systemImage: "bolt.fill")
                                    }.tint(.orange)
                                }
                            }
                            // Långtryck som alternativ till att svepa —
                            // touch-native mönster (Termius, Files.app m.fl.)
                            // som TestFlight-feedback 2026-07-28 efterfrågade.
                            // Samma handlingar som svep-menyerna ovan.
                            .contextMenu {
                                Button { editing = host } label: {
                                    Label("Ändra", systemImage: "pencil")
                                }
                                Button { model.toggleFavorite(host) } label: {
                                    Label(host.isFavorite ? "Ta bort favorit" : "Favorit",
                                          systemImage: host.isFavorite ? "star.slash" : "star")
                                }
                                if host.macAddress != nil {
                                    Button { wake(host) } label: {
                                        Label("Väck", systemImage: "bolt.fill")
                                    }
                                }
                                Button(role: .destructive) { model.delete(host) } label: {
                                    Label("Ta bort", systemImage: "trash")
                                }
                            }
                    }
                }
            }
        }
    }

    /// En tom värd för "+"-knappen. På iOS defaultar auth till lösenord —
    /// `.agentDefault` letar efter `~/.ssh/id_ed25519`, som inte finns på en
    /// sandlådad iPhone, så nya värdar kunde annars inte ansluta alls utan att
    /// användaren först grävde i auth-väljaren (TestFlight-feedback 2026-07-10).
    private static var newHost: Host {
        #if os(iOS)
        Host(alias: "", hostName: "", user: "", auth: .askPassword)
        #else
        Host(alias: "", hostName: "", user: "")
        #endif
    }

    private func start(_ host: Host) {
        if case .askPassword = host.auth {
            autofillUsername = host.user
            passwordFor = host
        } else {
            sessionManager.open(ConnectRequest(host: host, password: nil, initialCommand: host.startupCommand))
            showSessions = true
        }
    }
}

struct HostRow: View {
    let host: Host
    var body: some View {
        HStack(spacing: 12) {
            if let color = HostColorPalette.color(for: host.colorTag) {
                Circle().fill(color).frame(width: 10, height: 10)
            }
            Image(systemName: "terminal")
                .foregroundStyle(.secondary)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(host.alias.isEmpty ? host.hostName : host.alias)
                    .font(.body.weight(.medium))
                Text("\(host.user)@\(host.hostName):\(host.port)")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if host.isFavorite {
                Image(systemName: "star.fill").foregroundStyle(.yellow).font(.caption)
            }
        }
        .contentShape(Rectangle())
    }
}
#endif
