#if canImport(SwiftUI)
import SwiftUI
import SSHCore

/// Visas när man öppnar en värd: dashboard direkt, med knapp till terminalen.
struct HostDetailView: View {
    @Environment(\.dismiss) private var dismiss
    let request: ConnectRequest
    // Injicerad, INTE en egen `HostStore()`-instans — en fristående instans
    // läser en FÄRSK disksnapshot vid skapandet, och `upsert()` skriver
    // tillbaka HELA den snapshotten. Eftersom `HostDetailView` är ett
    // värdetyp-struct som SwiftUI kan återskapa ofta (varje gång
    // föräldravyn ritas om) skulle en egen instans läsa in en potentiellt
    // inaktuell kopia och sedan skriva över en samtidig ändring gjord via
    // HostListViews instans (CodeRabbit-fynd, #126). Samma instans som
    // HostListView.swift delas hela vägen ner via MultiSessionView istället.
    let store: HostStore
    /// Stänger (kopplar från) den här sessionen helt — skiljer sig från
    /// "Klar" (`dismiss()`), som bara tar bort den ur sikte och lämnar den
    /// ansluten i bakgrunden. `nil` när vyn inte ingår i en flikväxlare
    /// (bör inte hända i praktiken efter multisession-omskrivningen, men
    /// låter oss ändå inte kräva en anropare för varje instansiering).
    var onClose: (() -> Void)? = nil
    /// Öppnar ÄNNU en flik till SAMMA värd (t.ex. en `tail -f`-logg vid
    /// sidan av en vanlig shell) — synlig knapp istället för en gissad
    /// dubbeltryck-gest (TestFlight-feedback 2026-07-28: ingen flik
    /// upptäcktes alls eftersom SwiftUIs `TabView` döljer flikraden helt
    /// vid bara EN öppen session). `nil` när vyn inte ingår i en
    /// flikväxlare (samma resonemang som `onClose`).
    var onNewTab: (() -> Void)? = nil
    @State private var showTerminal = false
    @State private var reloadToken = UUID()

    var body: some View {
        NavigationStack {
            DashboardView(request: request, store: store)
                .id(reloadToken)   // ny id => DashboardView laddar om
                .navigationTitle(request.host.alias.isEmpty ? request.host.hostName : request.host.alias)
                .navInlineTitle()
                .safeAreaInset(edge: .bottom) {
                    Button { showTerminal = true } label: {
                        Label("Öppna terminal", systemImage: "terminal")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .padding()
                }
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Klar") { dismiss() }
                    }
                    ToolbarItem(placement: .primaryAction) {
                        Menu {
                            NavigationLink { DockerView(request: request, store: store) } label: {
                                Label("Docker", systemImage: "shippingbox")
                            }
                            NavigationLink { SnippetListView(request: request, store: store) } label: {
                                Label("Snippets", systemImage: "text.badge.checkmark")
                            }
                            NavigationLink { CommandLibraryView(request: request, store: store) } label: {
                                Label("Kommandobibliotek", systemImage: "books.vertical")
                            }
                            NavigationLink { SFTPBrowserView(request: request, store: store) } label: {
                                Label("Filer (SFTP)", systemImage: "folder")
                            }
                            NavigationLink { PortForwardView(request: request, store: store) } label: {
                                Label("Portvidarebefordran", systemImage: "arrow.left.arrow.right")
                            }
                            NavigationLink {
                                KeyDeployView(request: request, store: store) { updated in store.upsert(updated) }
                            } label: {
                                Label("SSH-nyckel", systemImage: "key")
                            }
                            if let onNewTab {
                                Button { onNewTab() } label: {
                                    Label("Ny flik till denna värd", systemImage: "plus.rectangle.on.rectangle")
                                }
                            }
                            if let onClose {
                                Button(role: .destructive) { onClose() } label: {
                                    Label("Stäng session", systemImage: "xmark.circle")
                                }
                            }
                        } label: {
                            Image(systemName: "ellipsis.circle")
                        }
                    }
                    ToolbarItem(placement: .primaryAction) {
                        Button { reloadToken = UUID() } label: { Image(systemName: "arrow.clockwise") }
                    }
                }
                .cover(isPresented: $showTerminal) {
                    SessionView(request: request, store: store)
                }
        }
    }
}
#endif
