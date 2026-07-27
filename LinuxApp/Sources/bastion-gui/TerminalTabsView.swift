import Foundation
import SSHCore
import SwiftCrossUI

/// Flera samtidiga terminalsessioner mot samma värd, växlingsbara via en
/// flikrad. Varje flik har sin egen `TerminalController` vars anslutning
/// (`start()`) körs som ett fristående `Task` skapat vid flik-skapande —
/// INTE via SwiftCrossUIs `.task`-modifier, som bara körs för vyer som
/// faktiskt renderas. Med `.task` hade en bakgrundsflik tappat sin
/// anslutning varje gång man växlade bort från den.
///
/// SwiftCrossUI saknar swipe-actions/DragGesture i sitt publika API (se
/// ROADMAP.md), så flikbyte sker via knapptryck i flikraden, inte genom att
/// svepa mellan fönster. Den saknar också en `.id()`-vymodifierare (till
/// skillnad från riktiga SwiftUI) — `State`s `update(with:previousValue:)`
/// återanvänder ALLTID föregående lagring oavsett vad konstruktorn nyss fick,
/// så ett underliggande `TerminalPane`-hjälpvy hade aldrig bytt vilken
/// controller den observerar. Lösningen: håll den AKTIVA controllern direkt
/// i `selectedController: TerminalController?` (Optional, inte en vanlig
/// klassreferens) och nolla-sen-sätt den vid flikbyte — `StateImpl.postSet()`
/// länkar bara om `didChange`-prenumerationen när värdet växlar mellan
/// `.none`/`.some` (se `OptionalObservableObject` i SwiftCrossUI), inte vid
/// ett direkt byte mellan två `.some`-värden.
struct TerminalTabsView: View {
    private struct Tab: Identifiable {
        let id = UUID()
        let controller: TerminalController
        let title: String
    }

    let host: Host
    let password: String?
    let initialCommand: String?
    let store: HostStore?

    @State private var tabs: [Tab]
    @State private var selectedTabID: UUID
    @State private var selectedController: TerminalController?
    @State private var nextNumber = 2

    init(host: Host, password: String?, initialCommand: String? = nil, store: HostStore? = nil) {
        self.host = host
        self.password = password
        self.initialCommand = initialCommand
        self.store = store
        let controller = TerminalController(host: host, password: password, initialCommand: initialCommand, store: store)
        let first = Tab(controller: controller, title: "1")
        self._tabs = State(wrappedValue: [first])
        self._selectedTabID = State(wrappedValue: first.id)
        self._selectedController = State(wrappedValue: controller)
        Task { @MainActor in await controller.start() }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            tabBar
            if let selectedController {
                TerminalPaneBody(controller: selectedController)
            }
        }
        .onDisappear {
            for tab in tabs { tab.controller.stop() }
        }
    }

    private var tabBar: some View {
        HStack(spacing: 6) {
            ForEach(tabs) { tab in
                HStack(spacing: 2) {
                    Button(tab.id == selectedTabID ? "● \(tab.title)" : tab.title) {
                        select(tab)
                    }
                    if tabs.count > 1 {
                        Button("×") { close(tab.id) }
                    }
                }
            }
            Button("+") { addTab() }
        }
    }

    private func select(_ tab: Tab) {
        guard tab.id != selectedTabID else { return }
        selectedTabID = tab.id
        selectedController = nil
        selectedController = tab.controller
    }

    private func addTab() {
        let controller = TerminalController(host: host, password: password, initialCommand: initialCommand, store: store)
        let tab = Tab(controller: controller, title: "\(nextNumber)")
        nextNumber += 1
        tabs.append(tab)
        Task { @MainActor in await controller.start() }
        select(tab)
    }

    private func close(_ id: UUID) {
        guard tabs.count > 1, let index = tabs.firstIndex(where: { $0.id == id }) else { return }
        tabs[index].controller.stop()
        tabs.remove(at: index)
        if selectedTabID == id {
            select(tabs[max(0, index - 1)])
        }
    }
}

/// Innehållet för en enskild flik: buffertvy, kontrolltangenter och
/// kommandorad. Ren renderingsvy — anslutningens livscykel ägs av
/// `TerminalTabsView`/fliken, inte av den här vyn.
private struct TerminalPaneBody: View {
    let controller: TerminalController
    @State private var input = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let status = controller.statusMessage {
                Text(status).foregroundColor(.red)
            }
            ScrollView {
                TerminalGridView(buffer: controller.buffer)
            }
            .frame(minHeight: 320)

            controlKeyRow

            HStack {
                TextField("Kommando…", text: $input)
                    .onSubmit { submit() }
                    .disabled(!controller.isActive)
                Button("Skicka") { submit() }
                    .disabled(input.isEmpty || !controller.isActive)
            }
        }
    }

    private func submit() {
        controller.sendLine(input)
        input = ""
    }

    private var controlKeyRow: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Button("Esc") { controller.sendRaw("\u{1B}") }
                Button("Tab") { controller.sendRaw("\t") }
                Button("←") { controller.sendRaw("\u{1B}[D") }
                Button("↑") { controller.sendRaw("\u{1B}[A") }
                Button("↓") { controller.sendRaw("\u{1B}[B") }
                Button("→") { controller.sendRaw("\u{1B}[C") }
                Button("Ctrl+C") { controller.sendRaw("\u{03}") }
                Button("Ctrl+D") { controller.sendRaw("\u{04}") }
            }
            HStack(spacing: 6) {
                Button("Home") { controller.sendRaw("\u{1B}[H") }
                Button("End") { controller.sendRaw("\u{1B}[F") }
                Button("PgUp") { controller.sendRaw("\u{1B}[5~") }
                Button("PgDn") { controller.sendRaw("\u{1B}[6~") }
                Button("Space") { controller.sendRaw(" ") }
            }
        }
    }
}
