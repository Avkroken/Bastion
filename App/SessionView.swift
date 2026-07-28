#if canImport(SwiftUI)
import SwiftUI
import SSHCore

/// Håller en aktiv terminalsession för en vald värd.
struct SessionView: View {
    @Environment(\.dismiss) private var dismiss
    let request: ConnectRequest
    /// För att slå upp en ev. jump-host, se `resolveConnectionPlan`. `nil`
    /// på anropsplatser utan delad store — bara en host UTAN jump-host
    /// ansluter då direkt; en host MED jumpHostID nekas anslutning
    /// (jump-hosten går inte att lösa upp utan store), se `resolveConnectionPlan`.
    var store: HostStore? = nil

    /// Resultatet av att slå upp anslutningsplanen: `.some` med ev. jump
    /// bara om ALLT som krävs kunde lösas. Se `resolveConnectionPlan`.
    private var plan: (auth: SSHAuth, jump: (target: SSHTarget, auth: SSHAuth)?)? {
        resolveConnectionPlan(for: request.host, password: request.password, store: store)
    }

    var body: some View {
        NavigationStack {
            Group {
                if let plan {
                    BastionTerminal(target: request.host.target, auth: plan.auth,
                                    jump: plan.jump,
                                    initialCommand: request.initialCommand,
                                    onSessionEnded: {
                                        // Neutralt: täcker BÅDE normal avslutning
                                        // (exit/Ctrl+D) OCH anslutningsfel
                                        // (CodeRabbit-fynd — påstod tidigare
                                        // felaktigt att det alltid var exit/Ctrl+D).
                                        debugLog("session", "fjärrshell avslutades för \(request.host.alias.isEmpty ? request.host.hostName : request.host.alias) — auto-stänger vyn")
                                        dismiss()
                                    })
                        .ignoresSafeArea(.container, edges: .bottom)
                        .background(Color.black)
                } else {
                    ContentUnavailableView(
                        "Kan inte autentisera", systemImage: "lock.slash",
                        description: Text("Kontrollera nyckelfil eller lösenord för värden (eller dess jump-host, om en är vald)."))
                }
            }
            .navigationTitle(request.host.alias.isEmpty ? request.host.hostName : request.host.alias)
            .navInlineTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Klar") { dismiss() }
                }
            }
        }
    }
}
#endif
