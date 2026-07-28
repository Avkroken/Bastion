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
    #if DEBUG
    /// Synligt live-diagnostik för svep-gesten, bara i DEBUG-bygget — så
    /// den som testar på riktig touchhårdvara kan se/skärmdumpa/rapportera
    /// exakt vad ett svep registrerade utan att behöva en Mac/Xcode
    /// uppkopplad samtidigt.
    @State private var lastSwipeDebug: String?
    #endif

    var body: some View {
        ZStack(alignment: .top) {
            TabView(selection: Binding(
                get: { manager.selectedID },
                set: { manager.selectedID = $0 }
            )) {
                ForEach(manager.sessions) { session in
                    HostDetailView(request: session, store: store, onClose: { manager.close(session.id) })
                        .tabItem {
                            Label(
                                session.host.alias.isEmpty ? session.host.hostName : session.host.alias,
                                systemImage: "terminal"
                            )
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
                        #if DEBUG
                        lastSwipeDebug = "svep dx=\(Int(horizontal)) dy=\(Int(vertical)) " +
                            (isHorizontal ? "→ flikbyte" : "→ ignorerad (lodrät)")
                        #endif
                        guard isHorizontal else { return }
                        selectAdjacent(offset: horizontal < 0 ? 1 : -1)
                    }
            )

            #if DEBUG
            if let lastSwipeDebug {
                Text(lastSwipeDebug)
                    .font(.caption)
                    .padding(6)
                    .background(.yellow.opacity(0.85))
                    .cornerRadius(6)
                    .padding(.top, 4)
            }
            #endif
        }
    }

    private func selectAdjacent(offset: Int) {
        guard let currentID = manager.selectedID,
              let index = manager.sessions.firstIndex(where: { $0.id == currentID })
        else { return }
        let target = index + offset
        guard manager.sessions.indices.contains(target) else { return }
        manager.selectedID = manager.sessions[target].id
    }
}
#endif
