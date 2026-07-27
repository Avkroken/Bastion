import Foundation
import SSHCore
import SwiftCrossUI

/// Toppnivå: värdlista i sidopanelen, dashboard/kommando i detaljvyn.
/// Motsvarar `App/HostListView.swift` + `App/HostDetailView.swift`
/// tillsammans, anpassat till `NavigationSplitView` utan iOS-sheets.
@MainActor struct ContentView: View {
    @State private var model = HostListModel()
    @State private var settings = SettingsModel()
    @State private var selectedHostID: UUID?
    @State private var editingHost: Host?
    @State private var showEditor = false
    @State private var showImport = false
    @State private var showWireGuard = false
    @State private var showTailscale = false
    @State private var showS3 = false
    @State private var showSettings = false
    @State private var searchText = ""
    @State private var wakeMessages: [UUID: String] = [:]
    @State private var showTelnetConnect = false
    @State private var pendingTelnetTarget: TelnetTarget?
    @State private var telnetTarget: TelnetTarget?
    @State private var showTelnetSession = false
    @State private var showQuickConnect = false
    @State private var pendingQuickConnectHost: Host?
    @State private var pendingQuickConnectPassword: String?
    @State private var quickConnectHost: Host?
    @State private var quickConnectPassword: String?
    @State private var showQuickConnectSession = false

    private var filteredHosts: [Host] {
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return model.hosts }
        let needle = trimmed.lowercased()
        return model.hosts.filter { host in
            host.alias.lowercased().contains(needle)
                || host.hostName.lowercased().contains(needle)
                || host.user.lowercased().contains(needle)
                || host.tags.contains { $0.lowercased().contains(needle) }
        }
    }

    var body: some View {
        NavigationSplitView {
            sidebar
        } detail: {
            if let host = model.hosts.first(where: { $0.id == selectedHostID }) {
                HostDetailView(host: host, store: model.store, toggles: settings.toggles, onHostUpdated: { model.save($0) })
            } else {
                ContentUnavailableView {
                    Text("Ingen värd vald")
                } description: {
                    Text("Välj en värd i listan, eller lägg till en ny.")
                }
            }
        }
        .sheet(isPresented: $showEditor) {
            if let editingHost {
                HostEditView(
                    host: editingHost,
                    allHosts: model.hosts,
                    onSave: { host in
                        model.save(host)
                        selectedHostID = host.id
                        showEditor = false
                    },
                    onCancel: { showEditor = false }
                )
            }
        }
        .sheet(isPresented: $showImport) {
            ImportConfigView(
                onImport: { text in
                    model.importConfig(text)
                    showImport = false
                },
                onCancel: { showImport = false }
            )
        }
        .sheet(isPresented: $showWireGuard) {
            WireGuardProfileListView()
        }
        .sheet(isPresented: $showTailscale) {
            TailscaleDiscoveryView(
                hosts: model.hosts,
                onAddHost: { alias, hostName in
                    editingHost = Host(alias: alias, hostName: hostName, user: "")
                    showTailscale = false
                    showEditor = true
                },
                onCancel: { showTailscale = false },
                store: model.store
            )
        }
        .sheet(isPresented: $showS3) {
            S3ConnectionListView()
        }
        .sheet(isPresented: $showSettings) {
            SettingsView(
                toggles: settings.toggles,
                onSave: { settings.save($0) },
                onClose: { showSettings = false }
            )
        }
        // Sätter INTE showTelnetSession direkt i onConnect — den sheeten är
        // fortfarande på väg att stängas då, och att öppna nästa sheet
        // samtidigt är samma krock som App/HostListView.swift redan
        // dokumenterar (CodeRabbit-fynd #115, cubic PR #173). Målet
        // mellanlagras i pendingTelnetTarget och befordras i
        // onChange(of: showTelnetConnect) istället för sheetens egen
        // onDismiss — SwiftCrossUIs SheetModifier kör INTE onDismiss vid
        // programmatisk isPresented=false (bara vid stängning via
        // fönsterramen), men onChange fyrar oavsett stängningsorsak
        // (cubic-fynd, denna PR).
        .sheet(isPresented: $showTelnetConnect) {
            TelnetConnectView(
                onConnect: { target in
                    pendingTelnetTarget = target
                    showTelnetConnect = false
                },
                onCancel: { showTelnetConnect = false }
            )
        }
        .onChange(of: showTelnetConnect) {
            guard !showTelnetConnect, let pending = pendingTelnetTarget else { return }
            pendingTelnetTarget = nil
            telnetTarget = pending
            showTelnetSession = true
        }
        // onDismiss OCH onClose nollar båda telnetTarget — onDismiss täcker
        // stängning via fönsterramen (onClose körs då inte), onClose täcker
        // "Klar"-knappen (onDismiss körs INTE vid det programmatiska
        // isPresented=false, se ovan) — utan båda läckte fjärrvärden/
        // lösenordet kvar i state beroende på stängningssätt (cubic-fynd).
        .sheet(isPresented: $showTelnetSession, onDismiss: { telnetTarget = nil }) {
            if let telnetTarget {
                TelnetSessionView(target: telnetTarget, onClose: {
                    showTelnetSession = false
                    self.telnetTarget = nil
                })
            }
        }
        .sheet(isPresented: $showQuickConnect) {
            QuickConnectView(
                onConnect: { host, password in
                    pendingQuickConnectHost = host
                    pendingQuickConnectPassword = password
                    showQuickConnect = false
                },
                onCancel: { showQuickConnect = false }
            )
        }
        .onChange(of: showQuickConnect) {
            guard !showQuickConnect, let pending = pendingQuickConnectHost else { return }
            pendingQuickConnectHost = nil
            quickConnectHost = pending
            quickConnectPassword = pendingQuickConnectPassword
            pendingQuickConnectPassword = nil
            showQuickConnectSession = true
        }
        .sheet(isPresented: $showQuickConnectSession, onDismiss: {
            quickConnectHost = nil
            quickConnectPassword = nil
        }) {
            if let quickConnectHost {
                VStack(alignment: .leading) {
                    HStack {
                        Text("Snabbanslutning").font(.headline)
                        Spacer()
                        Button("Klar") {
                            showQuickConnectSession = false
                            self.quickConnectHost = nil
                            self.quickConnectPassword = nil
                        }
                    }
                    TerminalSessionView(host: quickConnectHost, password: quickConnectPassword)
                }
                .padding()
            }
        }
        .onChange(of: selectedHostID) {
            if let hostID = selectedHostID {
                wakeMessages[hostID] = nil
            }
        }
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Button("Ny värd") {
                    editingHost = Host(alias: "", hostName: "", user: "")
                    showEditor = true
                }
                Button("Importera") { showImport = true }
                Button("WireGuard") { showWireGuard = true }
                Button("Tailscale") { showTailscale = true }
                Button("S3") { showS3 = true }
                Button("Telnet") { showTelnetConnect = true }
                Button("Snabbanslutning") { showQuickConnect = true }
                Spacer()
                Button("Inställningar") { showSettings = true }
            }

            TextField("Sök…", text: $searchText)

            List(filteredHosts, selection: $selectedHostID) { host in
                HStack {
                    if let color = HostColorPalette.color(for: host.colorTag) {
                        Circle().fill(color).frame(width: 10, height: 10)
                    }
                    VStack(alignment: .leading) {
                        Text(host.alias.isEmpty ? host.hostName : host.alias)
                        Text(host.tags.isEmpty ? "\(host.user)@\(host.hostName)" : host.tags.joined(separator: ", "))
                            .foregroundColor(.gray)
                    }
                    if host.isFavorite {
                        Text("★").foregroundColor(.yellow)
                    }
                }
            }

            if let selected = model.hosts.first(where: { $0.id == selectedHostID }) {
                HStack {
                    Button("Ändra") {
                        editingHost = selected
                        showEditor = true
                    }
                    Button(selected.isFavorite ? "★ Favorit" : "☆ Favorit") {
                        model.toggleFavorite(selected)
                    }
                    if selected.macAddress != nil {
                        Button("Väck") { wake(selected) }
                    }
                    Button("Ta bort") {
                        model.delete(selected)
                        selectedHostID = nil
                    }
                }
                if let message = wakeMessages[selected.id] {
                    Text(message).foregroundColor(.gray)
                }
            }
        }
        .padding()
        .frame(minWidth: 240)
    }

    /// Skickar ett magic packet till `host.macAddress` — best-effort, samma
    /// resonemang som App/HostListView.swift: fel visas inline istället för
    /// att sväljas tyst, men blockerar aldrig något annat i UI:t.
    private func wake(_ host: Host) {
        guard let mac = host.macAddress else { return }
        let hostID = host.id
        Task {
            do {
                try await WakeOnLan.send(mac: mac)
                wakeMessages[hostID] = "Skickade väckningssignal till \(host.alias.isEmpty ? host.hostName : host.alias)."
            } catch {
                wakeMessages[hostID] = "Kunde inte skicka väckningssignal: \(error)"
            }
        }
    }
}
