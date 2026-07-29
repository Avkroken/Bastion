import Foundation
import SwiftCrossUI

/// Klistra in en `~/.ssh/config` för att importera värdar. Motsvarar
/// `App/ImportConfigView.swift`.
struct ImportConfigView: View {
    @State private var text = ""
    let onImport: (String) -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Importera ssh-config").font(.headline)
            Text("Klistra in innehållet i din ~/.ssh/config nedan.")
                .foregroundColor(.gray)
            TextEditor(text: $text)
                .frame(minHeight: 200)
            HStack {
                Button("Avbryt") { onCancel() }
                Spacer()
                Button("Importera") { onImport(text) }
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding()
    }
}
