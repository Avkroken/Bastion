import SSHCore
import SwiftCrossUI

/// Visas när en värd väljs i sidopanelen: lösenordsgrind om det behövs,
/// annars dashboard + kommandokörning. Motsvarar `App/HostDetailView.swift`.
struct HostDetailView: View {
    let host: Host
    var store: HostStore? = nil
    var toggles: FeatureToggles = FeatureToggles()
    var onHostUpdated: (Host) -> Void = { _ in }
    @State private var password = ""
    @State private var connected = false
    @State private var resolvedPassword: String?
    @State private var showDocker = false
    @State private var showSnippets = false
    @State private var showCommandLibrary = false
    @State private var showSFTPBrowser = false
    @State private var showPortForward = false
    @State private var showKeyDeploy = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                VStack(alignment: .leading) {
                    Text(host.alias.isEmpty ? host.hostName : host.alias).font(.title2)
                    Text("\(host.user)@\(host.hostName):\(host.port)").foregroundColor(.gray)
                }
                Spacer()
                if connected || !needsPassword {
                    if toggles.showDocker {
                        Button("Docker") { showDocker = true }
                    }
                    if toggles.showSnippets {
                        Button("Snippets") { showSnippets = true }
                    }
                    if toggles.showCommandLibrary {
                        Button("Bibliotek") { showCommandLibrary = true }
                    }
                    if toggles.showSFTPBrowser {
                        Button("Filer") { showSFTPBrowser = true }
                    }
                    if toggles.showPortForward {
                        Button("Tunnlar") { showPortForward = true }
                    }
                    if toggles.showKeyDeploy {
                        Button("SSH-nyckel") { showKeyDeploy = true }
                    }
                }
            }

            if needsPassword && !connected {
                passwordGate
            } else {
                VStack(alignment: .leading, spacing: 16) {
                    DashboardView(host: host, password: resolvedPassword, store: store)
                    Divider()
                    TerminalTabsView(host: host, password: resolvedPassword, initialCommand: host.startupCommand, store: store)
                }
            }
        }
        .padding()
        .sheet(isPresented: $showDocker) {
            DockerView(host: host, password: resolvedPassword, store: store)
        }
        .sheet(isPresented: $showSnippets) {
            SnippetListView(host: host, password: resolvedPassword, hostStore: store)
        }
        .sheet(isPresented: $showCommandLibrary) {
            CommandLibraryView(host: host, password: resolvedPassword, hostStore: store)
        }
        .sheet(isPresented: $showSFTPBrowser) {
            SFTPBrowserView(host: host, password: resolvedPassword, store: store)
        }
        .sheet(isPresented: $showPortForward) {
            PortForwardView(host: host, password: resolvedPassword, store: store)
        }
        .sheet(isPresented: $showKeyDeploy) {
            KeyDeployView(host: host, password: resolvedPassword, store: store) { updated in
                onHostUpdated(updated)
                showKeyDeploy = false
            }
        }
    }

    private var needsPassword: Bool {
        if case .askPassword = host.auth { return true }
        return false
    }

    private var passwordGate: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Den här värden kräver lösenord.").foregroundColor(.gray)
            HStack {
                SecureField("Lösenord", text: $password)
                Button("Anslut") {
                    resolvedPassword = password
                    connected = true
                }
                .disabled(password.isEmpty)
            }
        }
        .padding(.top)
    }
}
