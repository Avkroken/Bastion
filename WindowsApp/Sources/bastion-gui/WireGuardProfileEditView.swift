import SSHCore
import SwiftCrossUI

/// Redigerar en `WireGuardProfile` som rå `.conf`-text — enklare och mer
/// direkt begripligt för en användare som redan har filen (från sin
/// VPN-leverantör/router) än ett fält-för-fält-formulär. `WireGuardConfig
/// (text:)` är förlåtande (okända rader hoppas tyst över, se dess
/// doc-kommentar) så ogiltig text ger bara en tom/ofullständig profil,
/// inte ett krasch eller en blockerande felruta.
struct WireGuardProfileEditView: View {
    let profile: WireGuardProfile
    let onSave: (WireGuardProfile) -> Void
    let onCancel: () -> Void

    @State private var name: String
    @State private var text: String

    init(profile: WireGuardProfile, onSave: @escaping (WireGuardProfile) -> Void, onCancel: @escaping () -> Void) {
        self.profile = profile
        self.onSave = onSave
        self.onCancel = onCancel
        self._name = State(wrappedValue: profile.name)
        self._text = State(wrappedValue: profile.config.rendered())
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("WireGuard-profil").font(.headline)
            TextField("Namn", text: $name)
            Text("Klistra in innehållet från en .conf-fil:").foregroundColor(.gray)
            TextEditor(text: $text)
            HStack {
                Button("Spara") {
                    let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
                    guard !trimmedName.isEmpty else { return }
                    var updated = profile
                    updated.name = trimmedName
                    updated.config = WireGuardConfig(text: text)
                    onSave(updated)
                }
                .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button("Avbryt") { onCancel() }
            }
        }
        .padding()
    }
}
