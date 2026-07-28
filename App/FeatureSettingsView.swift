#if canImport(SwiftUI)
import SwiftUI

/// Vilka valfria funktionsknappar/-kort som visas — klientbred inställning
/// (`UserDefaults`/`AppStorage`, inte per värd). Motsvarar LinuxApps
/// `FeatureToggles` (Sources/SSHCore/AppSettings.swift), men iOS-appen har
/// ingen delad Linux/Apple-modul för UI-inställningar, så det här är en
/// egen, minimal kopia — bara nycklarna, inget delat state.
///
/// TestFlight-feedback 2026-07-28: Dashboard-vyn visade Docker-kortet
/// ovillkorligt så fort en värd hade containrar — ingen möjlighet att dölja
/// det på en server utan Docker (eller där man bara inte bryr sig).
enum FeatureToggleKeys {
    static let showDocker = "featureShowDocker"
}

extension UserDefaults {
    /// `true` om nyckeln aldrig satts — annars hade ALLA befintliga
    /// installationer tappat Docker-kortet tyst vid uppgradering
    /// (samma resonemang som LinuxApps `FeatureToggles`-defaults).
    var showDockerCard: Bool {
        object(forKey: FeatureToggleKeys.showDocker) == nil
            ? true
            : bool(forKey: FeatureToggleKeys.showDocker)
    }
}

struct FeatureSettingsView: View {
    @Environment(\.dismiss) private var dismiss
    @AppStorage(FeatureToggleKeys.showDocker) private var showDocker = true

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Toggle("Docker-kort på dashboard", isOn: $showDocker)
                } footer: {
                    Text("Döljer Docker-kortet och container-informationen på dashboarden för värdar utan Docker installerat.")
                }
            }
            .navigationTitle("Funktioner")
            .navInlineTitle()
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Klar") { dismiss() }
                }
            }
        }
    }
}
#endif
