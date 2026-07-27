import Foundation
import Gtk
import GtkBackend
import SSHCore
import SwiftCrossUI

/// Flera samtidiga terminalsessioner mot samma värd, växlingsbara via en
/// flikrad. Varje flik har sin egen `TerminalController` vars anslutning
/// (`start()`) körs som ett fristående `Task`, oberoende av om fliken
/// renderas — annars hade en bakgrundsflik tappat sin anslutning varje gång
/// man växlade bort från den.
///
/// Den FÖRSTA fliken skapas/startas INTE i `init` — SwiftCrossUIs `@State`
/// överlever att vy-structen byggs om (varje gång FÖRÄLDERN, `HostDetailView`,
/// ritar om sin `body` av vilken anledning som helst, t.ex. Dashboard-
/// pollning, konstrueras en NY `TerminalTabsView`-instans och `init` körs
/// om), men den GAMLA `@State`-lagringen vinner ändå över den nya via
/// `StateImpl.update(with:previousValue:)`. Att starta en SSH-anslutning i
/// `init` hade alltså läckt en ny, omedelbart bortkastad anslutning vid
/// VARJE omritning (cubic/CodeRabbit-fynd, PR #214) — den skapade
/// controllern i just den körningen av `init` skrivs över och tappas, men
/// `Task { await controller.start() }` hann redan fyras på den. Lösningen:
/// bootstrappa första fliken i en `.task(id: host.id)` — dess EGNA interna
/// `@State` gör att den bara kör en gång per genuin värd-identitet (`onChange`
/// -semantik, inte per omritning), och avbryts korrekt av `.onDisappear`.
///
/// SwiftCrossUI saknar swipe-actions/DragGesture i sitt PUBLIKA API (se
/// ROADMAP.md) — flikbyte sker primärt via knapptryck. Äkta touchscreen-
/// svep finns ÄNDÅ: `.inspect(.onCreate)` (GtkBackend) ger oss vyns
/// underliggande `Gtk.Widget`, och `GestureSwipeBridge.swift` kopplar en
/// riktig `GtkGestureSwipe` på den via rå GLib-signaler (swift-cross-uis
/// egen `Gtk`-modul har ingen Swift-wrapper för just den gesten). Den saknar
/// också en `.id()`-vymodifierare (till
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
        // Svep vänster/höger byter flik — samma riktningskonvention som
        // sidbläddring på touchscreens (svep vänster = nästa, höger =
        // föregående). Tröskeln på hastighet + att horisontell rörelse
        // dominerar över vertikal filtrerar bort en vanlig lodrät
        // scroll-gest i terminalbufferten ovanför.
        .inspect(.onCreate) { [self] (widget: Gtk.Widget) in
            attachSwipeGesture(to: widget) { velocityX, velocityY in
                guard abs(velocityX) > abs(velocityY), abs(velocityX) > 200 else { return }
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
