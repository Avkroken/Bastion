import SSHCore
import SwiftCrossUI

/// Delar funktionstoggles (Docker, Snippets, ...) till vyerna, observerbart.
@MainActor
class SettingsModel: ObservableObject {
    let store = AppSettingsStore()
    @Published var toggles: FeatureToggles

    init() { toggles = store.current() }

    func save(_ newValue: FeatureToggles) {
        toggles = newValue
        store.update(newValue)
    }
}
