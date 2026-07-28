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
/// det på en server utan Docker (eller där man bara inte bryr sig). Samma
/// klagomål gäller principiellt hela värd-menyn i `HostDetailView` — sex
/// funktioner visas alltid, oavsett om man faktiskt använder dem.
enum FeatureToggleKeys {
    static let showDocker = "featureShowDocker"
    static let showSnippets = "featureShowSnippets"
    static let showCommandLibrary = "featureShowCommandLibrary"
    static let showSFTP = "featureShowSFTP"
    static let showPortForward = "featureShowPortForward"
    static let showKeyDeploy = "featureShowKeyDeploy"
}

private extension UserDefaults {
    /// Delad hjälpare: `true` om nyckeln aldrig satts (annars hade ALLA
    /// befintliga installationer tappat funktionen tyst vid uppgradering).
    func featureDefaultTrue(_ key: String) -> Bool {
        object(forKey: key) == nil ? true : bool(forKey: key)
    }
}

extension UserDefaults {
    var showDockerCard: Bool { featureDefaultTrue(FeatureToggleKeys.showDocker) }
    var showSnippetsMenuItem: Bool { featureDefaultTrue(FeatureToggleKeys.showSnippets) }
    var showCommandLibraryMenuItem: Bool { featureDefaultTrue(FeatureToggleKeys.showCommandLibrary) }
    var showSFTPMenuItem: Bool { featureDefaultTrue(FeatureToggleKeys.showSFTP) }
    var showPortForwardMenuItem: Bool { featureDefaultTrue(FeatureToggleKeys.showPortForward) }
    var showKeyDeployMenuItem: Bool { featureDefaultTrue(FeatureToggleKeys.showKeyDeploy) }
}

struct FeatureSettingsView: View {
    @Environment(\.dismiss) private var dismiss
    @AppStorage(FeatureToggleKeys.showDocker) private var showDocker = true
    @AppStorage(FeatureToggleKeys.showSnippets) private var showSnippets = true
    @AppStorage(FeatureToggleKeys.showCommandLibrary) private var showCommandLibrary = true
    @AppStorage(FeatureToggleKeys.showSFTP) private var showSFTP = true
    @AppStorage(FeatureToggleKeys.showPortForward) private var showPortForward = true
    @AppStorage(FeatureToggleKeys.showKeyDeploy) private var showKeyDeploy = true

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Toggle("Docker-kort på dashboard", isOn: $showDocker)
                } footer: {
                    Text("Döljer Docker-kortet och container-informationen på dashboarden för värdar utan Docker installerat.")
                }
                Section {
                    Toggle("Snippets", isOn: $showSnippets)
                    Toggle("Kommandobibliotek", isOn: $showCommandLibrary)
                    Toggle("Filer (SFTP)", isOn: $showSFTP)
                    Toggle("Portvidarebefordran", isOn: $showPortForward)
                    Toggle("SSH-nyckel", isOn: $showKeyDeploy)
                } header: {
                    Text("Värdmenyn")
                } footer: {
                    Text("Döljer funktioner du inte använder från per-värd-menyn (⋯). Ändras direkt, ingen omstart krävs.")
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
