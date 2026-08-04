#if canImport(SwiftUI)
import SwiftUI
import SSHCore

/// Flikväxlare mellan flera samtidigt anslutna värdar. `TabView` river inte
/// ner overksamma flikars vyer när man växlar (till skillnad från t.ex.
/// `NavigationStack`-push) — det är precis den egenskapen som håller
/// bakgrundssessioner faktiskt anslutna, utan någon egen livscykel-kod här.
///
/// Standard-`TabView` med `.tabItem` (flikbar-stilen, till skillnad från
/// `.page`-stilen) stödjer INTE svep mellan sidor på iOS — bara tryck på
/// flikbaren. Lagt till en egen `DragGesture` ovanpå för att stänga den
/// luckan (samma riktningskonvention som LinuxApp: svep vänster = nästa
/// flik, höger = föregående).
struct MultiSessionView: View {
    @ObservedObject var manager: SessionManager
    let store: HostStore

    var body: some View {
        TabView(selection: Binding(
            get: { manager.selectedID },
            set: { manager.selectedID = $0 }
        )) {
            ForEach(manager.sessions) { session in
                HostDetailView(
                    request: session,
                    store: store,
                    onClose: {
                        debugLog("tabs", "stänger flik för \(displayLabel(for: session))")
                        manager.close(session.id)
                    },
                    onNewTab: {
                        let new = ConnectRequest(host: session.host, password: session.password, initialCommand: nil)
                        manager.open(new)
                        debugLog("tabs", "ny flik till \(displayLabel(for: session)) — nu \(manager.sessions.count) flikar totalt")
                    }
                )
                    .tabItem {
                        Label(displayLabel(for: session), systemImage: "terminal")
                    }
                    .tag(Optional(session.id))
            }
        }
        .gesture(
            DragGesture(minimumDistance: 40)
                .onEnded { value in
                    let horizontal = value.translation.width
                    let vertical = value.translation.height
                    let isHorizontal = abs(horizontal) > abs(vertical)
                    debugLog("gesture", "svep dx=\(Int(horizontal)) dy=\(Int(vertical)) " +
                        (isHorizontal ? "→ flikbyte" : "→ ignorerad (lodrät rörelse dominerar)"))
                    guard isHorizontal else { return }
                    selectAdjacent(offset: horizontal < 0 ? 1 : -1)
                }
        )
    }

    /// Flikar mot SAMMA värd (via "Ny flik till denna värd") ser annars
    /// identiska ut i flikraden — numrerar dem (2), (3) osv. i den ordning
    /// de finns i `manager.sessions`.
    private func displayLabel(for session: ConnectRequest) -> String {
        let base = session.host.alias.isEmpty ? session.host.hostName : session.host.alias
        let sameHost = manager.sessions.filter { $0.host.id == session.host.id }
        guard sameHost.count > 1, let index = sameHost.firstIndex(where: { $0.id == session.id }) else {
            return base
        }
        return index == 0 ? base : "\(base) (\(index + 1))"
    }

    private func selectAdjacent(offset: Int) {
        guard let currentID = manager.selectedID,
              let index = manager.sessions.firstIndex(where: { $0.id == currentID })
        else { return }
        let target = index + offset
        guard manager.sessions.indices.contains(target) else { return }
        manager.selectedID = manager.sessions[target].id
        debugLog("tabs", "växlade till flik \(target + 1)/\(manager.sessions.count): \(displayLabel(for: manager.sessions[target]))")
    }
}
#endif
