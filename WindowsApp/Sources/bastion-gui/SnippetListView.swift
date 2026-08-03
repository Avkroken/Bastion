import Foundation
import SSHCore
import SwiftCrossUI

@MainActor
final class SnippetListModel: ObservableObject {
    private let store = SnippetStore()
    @Published var snippets: [Snippet] = []

    init() { reload() }
    func reload() { snippets = store.all() }
    func save(_ snippet: Snippet) { store.upsert(snippet); reload() }
    func delete(_ snippet: Snippet) { store.delete(snippet.id); reload() }
}

/// Lista över sparade snippets — motsvarar `App/SnippetListView.swift`. Kör
/// en snippet genom att öppna en ny terminal med det renderade kommandot som
/// startkommando (samma mönster som Docker-vyns Shell-knapp).
struct SnippetListView: View {
    let host: Host
    let password: String?
    var hostStore: HostStore? = nil
    @State private var model = SnippetListModel()
    @State private var editingSnippet: Snippet?
    @State private var showEditor = false
    @State private var runCommand: String?
    @State private var selectedSnippetID: UUID?

    private var selectedSnippet: Snippet? {
        model.snippets.first { $0.id == selectedSnippetID }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Snippets").font(.title2)
                Spacer()
                Button("Nytt") {
                    editingSnippet = Snippet(name: "", template: "")
                    showEditor = true
                }
            }

            if model.snippets.isEmpty {
                Text("Inga snippets än — lägg till ett kommando med variabler, t.ex. \"docker compose restart {{service}}\".")
                    .foregroundColor(.gray)
            } else {
                // Tryck en rad för att välja den — kör-formuläret nedanför
                // reagerar på selectedSnippetID. Ingen Button per rad
                // (SwiftCrossUIs List-rader är redan tryckbara via selection,
                // och Button stödjer inte en godtycklig vy som label).
                List(model.snippets, id: \.id, selection: $selectedSnippetID) { snippet in
                    VStack(alignment: .leading) {
                        Text(snippet.name)
                        Text(snippet.template).foregroundColor(.gray)
                    }
                }

                if let selected = selectedSnippet {
                    HStack {
                        Button("Ändra") { editingSnippet = selected; showEditor = true }
                        Button("Ta bort") { model.delete(selected); selectedSnippetID = nil }
                    }
                }
            }

            if let selected = selectedSnippet {
                snippetRunForm(for: selected)
            }
        }
        .padding()
        .sheet(isPresented: $showEditor) {
            if let editingSnippet {
                SnippetEditView(
                    snippet: editingSnippet,
                    onSave: { model.save($0); showEditor = false },
                    onCancel: { showEditor = false }
                )
            }
        }
        // Samma CodeRabbit-fynd som CommandLibraryView.swift (PR #33): rensa
        // variableValues vid varje nytt radval, inte bara via Avbryt/Kör —
        // annars kan ett ifyllt värde smyga med till ett annat snippet med
        // samma variabelnamn.
        .onChange(of: selectedSnippetID) { variableValues = [:] }
        .sheet(isPresented: Binding(get: { runCommand != nil }, set: { if !$0 { runCommand = nil } })) {
            TerminalSessionView(host: host, password: password, initialCommand: runCommand, store: hostStore)
        }
    }

    private func snippetRunForm(for snippet: Snippet) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Divider()
            Text("Kör: \(snippet.name)").font(.headline)
            if snippet.variableNames.isEmpty {
                Text(snippet.template).foregroundColor(.gray)
            } else {
                ForEach(snippet.variableNames, id: \.self) { name in
                    TextField(name, text: variableBinding(name))
                }
            }
            HStack {
                Button("Avbryt") { selectedSnippetID = nil }
                Button("Kör") {
                    runCommand = snippet.rendered(with: variableValues)
                    selectedSnippetID = nil
                    variableValues = [:]
                }
            }
        }
    }

    @State private var variableValues: [String: String] = [:]
    private func variableBinding(_ name: String) -> Binding<String> {
        Binding(get: { variableValues[name] ?? "" }, set: { variableValues[name] = $0 })
    }
}
