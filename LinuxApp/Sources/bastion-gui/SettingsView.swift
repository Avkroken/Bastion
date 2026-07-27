import SSHCore
import SwiftCrossUI

/// Låter användaren slå av/på valfria funktionsknappar per klient (t.ex.
/// dölja Docker på en värd utan Docker installerat). Sparas i
/// `~/.bastion/settings.json` via `AppSettingsStore`, oberoende av
/// värddatabasen och andra plattformar.
struct SettingsView: View {
    @State private var toggles: FeatureToggles
    let errorMessage: String?
    /// Returnerar `true` om sparningen lyckades — vyn stänger sig bara då,
    /// annars förblir den öppen med felmeddelandet synligt.
    let onSave: (FeatureToggles) -> Bool
    let onClose: () -> Void

    init(
        toggles: FeatureToggles,
        errorMessage: String? = nil,
        onSave: @escaping (FeatureToggles) -> Bool,
        onClose: @escaping () -> Void
    ) {
        self._toggles = State(wrappedValue: toggles)
        self.errorMessage = errorMessage
        self.onSave = onSave
        self.onClose = onClose
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Inställningar").font(.title2)
            Text("Visa/dölj funktionsknappar för alla värdar på den här klienten. Praktiskt om t.ex. Docker inte är installerat någonstans.")
                .foregroundColor(.gray)

            Toggle("Docker", isOn: $toggles.showDocker)
            Toggle("Snippets", isOn: $toggles.showSnippets)
            Toggle("Kommandobibliotek", isOn: $toggles.showCommandLibrary)
            Toggle("Filer (SFTP)", isOn: $toggles.showSFTPBrowser)
            Toggle("Tunnlar", isOn: $toggles.showPortForward)
            Toggle("SSH-nyckeldistribution", isOn: $toggles.showKeyDeploy)

            if let errorMessage {
                Text(errorMessage).foregroundColor(.red)
            }

            HStack {
                Spacer()
                Button("Klar") {
                    if onSave(toggles) {
                        onClose()
                    }
                }
            }
        }
        .padding()
    }
}
