import SSHCore
import SwiftCrossUI

/// Delar host-databasen till vyerna och håller den observerbar.
/// Samma roll som `App/HostListView.swift`s `HostListModel`.
@MainActor
class HostListModel: ObservableObject {
    let store = HostStore()
    @Published var hosts: [Host] = []

    init() { reload() }
    /// Favoriter först, sedan alfabetiskt inom respektive grupp — ingen
    /// tagg-gruppering här (till skillnad från App/), så det är enda
    /// sorteringssignalen utöver alias.
    func reload() {
        hosts = store.all().sorted {
            if $0.isFavorite != $1.isFavorite { return $0.isFavorite }
            return $0.alias.lowercased() < $1.alias.lowercased()
        }
    }
    func save(_ host: Host) { store.upsert(host); reload() }
    func delete(_ host: Host) { store.delete(host.id); reload() }
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
}
