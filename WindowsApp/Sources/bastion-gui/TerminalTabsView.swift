import Foundation
import SSHCore
import SwiftCrossUI
import WinUI

/// Windows-motsvarigheten till `LinuxApp/Sources/bastion-gui/TerminalTabsView.swift`
/// — identisk logik, bara svep-bryggan bytt ut mot WinUI-varianten
/// (`WinUISwipeGestureBridge.swift`, samma mönster som redan verifierat i
/// `BastionGUIApp.swift`s tidigare platshållar-demo). Se LinuxApp-filens
/// kommentarer för resonemanget bakom `.task(id:)`-bootstrap och
/// `selectedController`-hanteringen — det är delad SwiftCrossUI-logik,
/// inte något som ändrats här.
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

    @State private var tabs: [Tab] = []
    @State private var selectedTabID: UUID?
    @State private var selectedController: TerminalController?
    @State private var nextNumber = 2

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            tabBar
            if let selectedController {
                TerminalPaneBody(controller: selectedController)
            }
        }
        .task(id: host.id) {
            guard tabs.isEmpty else { return }
            let controller = TerminalController(host: host, password: password, initialCommand: initialCommand, store: store)
            let first = Tab(controller: controller, title: "1")
            tabs = [first]
            selectedTabID = first.id
            selectedController = controller
            await controller.start()
        }
        .onDisappear {
            for tab in tabs { tab.controller.stop() }
        }
        // Svep vänster/höger byter flik, samma tröskel/riktningskonvention
        // som LinuxApp-varianten (GTK: rå GLib-signal, här: swift-winuis
        // egen WinRT-projektion — se WinUISwipeGestureBridge.swift för
        // varför Windows inte behöver en rå ABI-koppling som GTK gör).
        .inspect(.onCreate) { [self] (element: WinUI.FrameworkElement) in
            attachSwipeGesture(to: element) { velocityX, velocityY in
                guard abs(velocityX) > abs(velocityY), abs(velocityX) > 0.2 else { return }
                selectAdjacent(offset: velocityX < 0 ? 1 : -1)
            }
        }
    }

    private func selectAdjacent(offset: Int) {
        guard let currentID = selectedTabID,
              let index = tabs.firstIndex(where: { $0.id == currentID })
        else { return }
        let target = index + offset
        guard tabs.indices.contains(target) else { return }
        select(tabs[target])
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
