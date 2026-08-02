#if canImport(SwiftTerm) && (os(iOS) || os(macOS))
import SwiftTerm
import SwiftUI
import SSHCore
import Foundation
#if os(iOS)
import UIKit
#else
import AppKit
#endif

// XCODE-ONLY. Byggs inte av SwiftPM på Linux (SwiftTerm kräver UIKit/AppKit).
// Lägg till SwiftTerm som paketberoende i Xcode:
//   https://github.com/migueldeicaza/SwiftTerm  (MIT)
//
// Kopplar SSHCore.SSHShell (interaktiv PTY-shell) till en riktig SwiftTerm-vy:
//   fjärr-stdout  -> terminalView.feed
//   tangenttryck  -> shell.send
//   storleksändr. -> shell.resize
//
// Not: TerminalViewDelegate-protokollet har fler metoder i vissa SwiftTerm-
// versioner (clipboardCopy, requestOpenLink, bell, iTermContent …). Lägg till
// tomma stubbar för dem som din version kräver — kärnkopplingen nedan är den
// som betyder något.

/// Version-oberoende koppling: äger anslutningen och shellen, pumpar utdata
/// till en sink och tar emot tangenttryck/storlek. Testbar utan UI.
@MainActor
final class SSHTerminalController {
    private let target: SSHTarget
    private let auth: SSHAuth
    /// Om satt: kopplas målet GENOM denna jump-host (ssh -J/ProxyJump) —
    /// se `Host.jumpHostID` och `SSHConnectionChain`. `nil` = direkt
    /// anslutning, precis som innan jump-stöd fanns.
    private let jump: (target: SSHTarget, auth: SSHAuth)?
    private var chain: SSHConnectionChain?
    private var shell: SSHShell?
    /// Sätts av stop(). Kollas efter varje await-punkt i start() så en sen
    /// connect()/openShell() som landar EFTER teardown stänger det den just
    /// öppnade istället för att bli en föräldralös, aldrig stängd session
    /// (CodeRabbit-fynd på #155: stop() stänger bara det som redan hunnit
    /// tilldelas self.chain/self.shell VID ANROPSTILLFÄLLET).
    private var isStopped = false

    /// Anropas på main med bytes att mata in i terminalvyn.
    var onData: ((ArraySlice<UInt8>) -> Void)?
    /// Skickas till shellen direkt efter att den öppnats (t.ex. `docker exec …`).
    var initialCommand: String?

    init(target: SSHTarget, auth: SSHAuth, jump: (target: SSHTarget, auth: SSHAuth)? = nil, initialCommand: String? = nil) {
        self.target = target
        self.auth = auth
        self.jump = jump
        self.initialCommand = initialCommand
    }

    func start(cols: Int, rows: Int) {
        Task {
            do {
                let chain = try await SSHConnectionChain.connect(target: target, targetAuth: auth, jump: jump)
                self.chain = chain
                guard !isStopped else { await chain.close(); return }
                let shell = try await chain.target.openShell(cols: cols, rows: rows)
                guard !isStopped else { shell.close(); return }
                self.shell = shell
                // Håller anslutningen vaken genom NAT/brandväggars idle-timeout
                // (se SSHShell.startKeepAlive) — stoppas automatiskt av
                // shell.close() i stop().
                shell.startKeepAlive()
                if let cmd = initialCommand { shell.send(cmd + "\n") }
                for try await chunk in shell.output {
                    guard !isStopped else { break }
                    let bytes = chunk.bytes
                    self.onData?(bytes[...])
                }
                // Strömmen tar slut normalt när fjärrshellen stänger (t.ex.
                // `exit`) — måste städas här precis som i catch-grenen
                // nedan, annars förblir keepAlive-Task:en och den underliggande
                // anslutningen aktiva utan att någon någonsin river ner dem
                // (CodeRabbit-fynd: den här grenen saknade helt städning,
                // till skillnad från LinuxApp/WindowsApp-motsvarigheterna).
                self.shell?.close()
                await self.chain?.close()
            } catch {
                // Om felet kom EFTER att chain redan var uppsatt (openShell()
                // eller output-strömmen misslyckades, inte själva anslutningen)
                // måste den städas här — SSHConnectionChain.connect() städar
                // bara sina EGNA fel internt, inte fel som inträffar efter att
                // den redan returnerat. Ofarligt no-op om chain fortfarande är
                // nil (connect() self själv redan städat i den vägen).
                // self.shell?.close() FÖRE chain?.close() — stoppar keepAlive-
                // Task:en innan chain.close() river ner event loop-gruppen
                // under den (CodeRabbit-fynd), samma race-klass som redan
                // dokumenteras i SSHSession.swift.
                self.shell?.close()
                await self.chain?.close()
                guard !isStopped else { return }
                let msg = Array("\r\n[bastion] fel: \(error)\r\n".utf8)
                self.onData?(msg[...])
            }
        }
    }

    func sendKeys(_ data: ArraySlice<UInt8>) { shell?.send(Array(data)) }
    func resize(cols: Int, rows: Int) { shell?.resize(cols: cols, rows: rows) }
    func stop() {
        isStopped = true
        shell?.close()
        let chain = self.chain
        Task { await chain?.close() }
    }
}

#if os(iOS)
private typealias TTColor = UIColor
#else
private typealias TTColor = NSColor
#endif

private extension SwiftTerm.Color {
    /// Bygger en SwiftTerm-färg ur en "#RRGGBB"-hexsträng via den delade
    /// `HexRGB`-parsern (TerminalTheme.swift). SwiftTerm.Color-komponenter
    /// är 0-65535, så 0-1-komponenterna skalas upp med 65535.
    convenience init(hex: String) {
        let rgb = HexRGB(hex)
        self.init(red: UInt16(rgb.red * 65535), green: UInt16(rgb.green * 65535), blue: UInt16(rgb.blue * 65535))
    }
}

private extension TTColor {
    /// SwiftTerms egen `TTColor`/`.make(color:)` är interna (utan `public`)
    /// i SwiftTerm-modulen, alltså oåtkomliga härifrån — bygger istället
    /// direkt mot UIColor/NSColor via samma delade `HexRGB`-parser.
    convenience init(hex: String) {
        let rgb = HexRGB(hex)
        self.init(red: CGFloat(rgb.red), green: CGFloat(rgb.green), blue: CGFloat(rgb.blue), alpha: 1.0)
    }
}

extension TerminalView {
    /// Applicerar ett Bastion-terminaltema: bakgrund/text/markör/markering +
    /// de 16 ANSI-färgerna. `installColors` uppdaterar både färgmotorn och
    /// om-renderar existerande innehåll (se SwiftTerm.TerminalView).
    func apply(theme: TerminalTheme) {
        nativeBackgroundColor = TTColor(hex: theme.background)
        nativeForegroundColor = TTColor(hex: theme.foreground)
        caretColor = TTColor(hex: theme.cursor)
        selectedTextBackgroundColor = TTColor(hex: theme.selection)
        installColors(theme.ansi.map { SwiftTerm.Color(hex: $0) })
    }
}

#if os(iOS)
typealias TerminalRepresentable = UIViewRepresentable
#else
typealias TerminalRepresentable = NSViewRepresentable
#endif

struct BastionTerminal: TerminalRepresentable {
    let target: SSHTarget
    let auth: SSHAuth
    /// Se `SSHTerminalController.jump` — `nil` = direkt anslutning.
    var jump: (target: SSHTarget, auth: SSHAuth)? = nil
    var initialCommand: String? = nil

    func makeCoordinator() -> Coordinator {
        Coordinator(target: target, auth: auth, jump: jump, initialCommand: initialCommand)
    }

    private func build(_ context: Context) -> TerminalView {
        let view = TerminalView()
        view.terminalDelegate = context.coordinator
        context.coordinator.attach(view)
        return view
    }

    #if os(iOS)
    func makeUIView(context: Context) -> TerminalView { build(context) }
    func updateUIView(_ uiView: TerminalView, context: Context) {}
    static func dismantleUIView(_ uiView: TerminalView, coordinator: Coordinator) {
        coordinator.tearDown()
    }
    #else
    func makeNSView(context: Context) -> TerminalView { build(context) }
    func updateNSView(_ nsView: TerminalView, context: Context) {}
    static func dismantleNSView(_ nsView: TerminalView, coordinator: Coordinator) {
        coordinator.tearDown()
    }
    #endif

    @MainActor
    final class Coordinator: NSObject, TerminalViewDelegate {
        private let controller: SSHTerminalController
        private weak var view: TerminalView?

        init(target: SSHTarget, auth: SSHAuth, jump: (target: SSHTarget, auth: SSHAuth)?, initialCommand: String?) {
            self.controller = SSHTerminalController(target: target, auth: auth, jump: jump, initialCommand: initialCommand)
            super.init()
            controller.onData = { [weak self] bytes in
                self?.view?.feed(byteArray: bytes)
            }
        }

        func attach(_ view: TerminalView) {
            self.view = view
            let savedID = UserDefaults.standard.string(forKey: TerminalThemeKeys.selectedID)
            view.apply(theme: TerminalTheme.theme(id: savedID))
            let t = view.getTerminal()
            controller.start(cols: t.cols, rows: t.rows)
        }

        /// Anropas av dismantleUIView/dismantleNSView när vyn tas bort ur
        /// hierarkin. Utan denna fortsätter bakgrunds-Task:en i controller.start()
        /// köra och mata feed() på en föräldralös vy efter dismiss — till skillnad
        /// från PortForwardView/DockerView/SFTPBrowserView, som redan städar via
        /// .onDisappear { model.disconnect() }.
        func tearDown() {
            controller.stop()
            view = nil
        }

        // Tangenttryck från terminalen -> fjärr-shell.
        func send(source: TerminalView, data: ArraySlice<UInt8>) {
            controller.sendKeys(data)
        }

        // Terminalen ändrade storlek -> meddela fjärrsidan.
        func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {
            controller.resize(cols: newCols, rows: newRows)
        }

        func setTerminalTitle(source: TerminalView, title: String) {}
        func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {}
        func scrolled(source: TerminalView, position: Double) {}
        func clipboardCopy(source: TerminalView, content: Data) {}
        func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {}
        func rangeChanged(source: TerminalView, startY: Int, endY: Int) {}
    }
}
#endif
