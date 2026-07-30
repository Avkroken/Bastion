import Foundation
import SSHCore
import SwiftCrossUI

@MainActor
final class WireGuardProfileListModel: ObservableObject {
    private let store = WireGuardProfileStore()
    @Published var profiles: [WireGuardProfile] = []

    init() { reload() }
    func reload() { profiles = store.all() }
    func save(_ profile: WireGuardProfile) { store.upsert(profile); reload() }
    func delete(_ profile: WireGuardProfile) { store.delete(profile.id); reload() }
}

/// Lista över sparade WireGuard-profiler (toppnivå, inte kopplat till en
/// specifik `Host` — en profil beskriver en VPN-anslutning, inte en SSH-värd).
/// v1: profilhantering (importera/visa/redigera/ta bort `.conf`-text), INTE
/// att faktiskt upprätta tunneln — se `WireGuardConfig.swift`s doc-kommentar
/// för varför det är avgränsat så.
struct WireGuardProfileListView: View {
    @State private var model = WireGuardProfileListModel()
    @State private var editingProfile: WireGuardProfile?
    @State private var showEditor = false
    @State private var selectedProfileID: UUID?

    private var selectedProfile: WireGuardProfile? {
        model.profiles.first { $0.id == selectedProfileID }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("WireGuard-profiler").font(.title2)
                Spacer()
                Button("Importera") {
                    editingProfile = WireGuardProfile(name: "", config: WireGuardConfig())
                    showEditor = true
                }
            }
            Text("Profilhantering — sparar och redigerar .conf-konfigurationer. Upprättar inte tunneln än (kräver wg-binären).")
                .foregroundColor(.gray)

            if model.profiles.isEmpty {
                Text("Inga profiler än — klistra in en .conf-fils innehåll för att spara en.")
                    .foregroundColor(.gray)
            } else {
                List(model.profiles, id: \.id, selection: $selectedProfileID) { profile in
                    VStack(alignment: .leading) {
                        Text(profile.name)
                        Text(profileSummary(profile.config)).foregroundColor(.gray)
                    }
                }

                if let selected = selectedProfile {
                    HStack {
                        Button("Ändra") { editingProfile = selected; showEditor = true }
                        Button("Ta bort") { model.delete(selected); selectedProfileID = nil }
                    }
                }
            }
        }
        .padding()
        .sheet(isPresented: $showEditor) {
            if let editingProfile {
                WireGuardProfileEditView(
                    profile: editingProfile,
                    onSave: { model.save($0); showEditor = false },
                    onCancel: { showEditor = false }
                )
            }
        }
    }

    private func profileSummary(_ config: WireGuardConfig) -> String {
        let address = config.interface.address.first ?? "ingen adress"
        let peerWord = config.peers.count == 1 ? "peer" : "peers"
        return "\(address) · \(config.peers.count) \(peerWord)"
    }
}
