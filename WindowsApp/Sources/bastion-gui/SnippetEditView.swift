import Foundation
import SSHCore
import SwiftCrossUI

/// Lägg till / ändra ett snippet. Motsvarar `App/SnippetEditView.swift`.
struct SnippetEditView: View {
    @State private var draft: Snippet
    let onSave: (Snippet) -> Void
    let onCancel: () -> Void

    init(snippet: Snippet, onSave: @escaping (Snippet) -> Void, onCancel: @escaping () -> Void) {
        self._draft = State(wrappedValue: snippet)
        self.onSave = onSave
        self.onCancel = onCancel
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(draft.name.isEmpty ? "Nytt snippet" : "Ändra snippet").font(.headline)

            TextField("Namn (t.ex. Starta om Plex)", text: $draft.name)
            TextField("Kommando — {{variabel}} för ifyllbara delar", text: $draft.template)

            if !draft.variableNames.isEmpty {
                Text("Upptäckta variabler: " + draft.variableNames.joined(separator: ", "))
                    .foregroundColor(.gray)
            }

            HStack {
                Button("Avbryt") { onCancel() }
                Spacer()
                Button("Spara") { onSave(draft) }.disabled(!isValid)
            }
        }
        .padding()
    }

    private var isValid: Bool {
        !draft.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !draft.template.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}
