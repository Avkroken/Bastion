import SSHCore
import SwiftCrossUI

/// Delar funktionstoggles (Docker, Snippets, ...) till vyerna, observerbart.
@MainActor
class SettingsModel: ObservableObject {
    let store = AppSettingsStore()
    @Published var toggles: FeatureToggles
    @Published var saveError: String?

    init() { toggles = store.current() }

    /// Publicerar det nya värdet bara om det faktiskt gick att spara —
    /// annars hade GUI:t visat en inställning som reverterar tyst till den
    /// gamla vid nästa omstart, utan att användaren fått veta varför.
    @discardableResult
    func save(_ newValue: FeatureToggles) -> Bool {
        do {
            try store.update(newValue)
            toggles = newValue
            saveError = nil
            return true
        } catch {
            saveError = "Kunde inte spara inställningarna: \(error)"
            return false
        }
    }
}
