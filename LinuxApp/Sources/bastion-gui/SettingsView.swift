import SSHCore
import SwiftCrossUI

/// Låter användaren slå av/på valfria funktionsknappar per klient (t.ex.
/// dölja Docker på en värd utan Docker installerat). Sparas i
/// `~/.bastion/settings.json` via `AppSettingsStore`, oberoende av
/// värddatabasen och andra plattformar.
struct SettingsView: View {
    @State private var toggles: FeatureToggles
    let onSave: (FeatureToggles) -> Void
    let onClose: () -> Void

    init(toggles: FeatureToggles, onSave: @escaping (FeatureToggles) -> Void, onClose: @escaping () -> Void) {
        self._toggles = State(wrappedValue: toggles)
        self.onSave = onSave
        self.onClose = onClose
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Inställningar").font(.title2)
            Text("Visa/dölj funktionsknappar per värd. Praktiskt om t.ex. Docker inte är installerat.")
                .foregroundColor(.gray)

            Toggle("Docker", isOn: $toggles.showDocker)
            Toggle("Snippets", isOn: $toggles.showSnippets)
            Toggle("Kommandobibliotek", isOn: $toggles.showCommandLibrary)
            Toggle("Filer (SFTP)", isOn: $toggles.showSFTPBrowser)
            Toggle("Tunnlar", isOn: $toggles.showPortForward)
            Toggle("SSH-nyckeldistribution", isOn: $toggles.showKeyDeploy)

            HStack {
                Spacer()
                Button("Klar") {
                    onSave(toggles)
                    onClose()
                }
            }
        }
        .padding()
    }
}
