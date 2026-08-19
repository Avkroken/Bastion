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
    /// Garanterar att den FAKTISKA nedstängningen (stänga `shell`, stänga
    /// `chain`) bara körs EN gång oavsett vilken av de tre vägarna som
    /// hinner först: `stop()`, output-strömmens normala slut, eller ett
    /// fel i `start()`s Task. Utan detta kunde två vägar råka trigga
    /// samtidigt (t.ex. `stop()` medan output-loopen precis avslutas
    /// naturligt) och båda anropa `shell.close()` + starta överlappande
    /// `chain.close()`-anrop (CodeRabbit-fynd). Säkert att kolla/sätta
    /// synkront utan lås — allt här körs på `@MainActor`, och kollen+
    /// sättningen i `teardown()` sker ALDRIG över en `await`-punkt.
    private var isTornDown = false

    /// Anropas på main med bytes att mata in i terminalvyn.
    var onData: ((ArraySlice<UInt8>) -> Void)?
    /// Anropas EXAKT en gång när fjärrshellen stänger — antingen normalt
    /// (`exit`/Ctrl+D, output-strömmen tar slut utan fel) eller via ett fel.
    /// INTE vid `stop()` (använraren stängde själv, redan hanterat av
    /// anroparen). Låter SessionView auto-stänga terminalvyn istället för
    /// att lämna en tyst död session som kräver ett manuellt tryck på
    /// "Klar" (TestFlight-feedback 2026-07-28).
    var onSessionEnded: (() -> Void)?
    /// Skickas till shellen direkt efter att den öppnats (t.ex. `docker exec …`).
    var initialCommand: String?

    init(target: SSHTarget, auth: SSHAuth, jump: (target: SSHTarget, auth: SSHAuth)? = nil, initialCommand: String? = nil) {
        self.target = target
        self.auth = auth
        self.jump = jump
        self.initialCommand = initialCommand
    }

    func start(cols: Int, rows: Int) {
        debugLog("session", "start() target=\(target.host):\(target.port) cols=\(cols) rows=\(rows)")
        Task {
            do {
                let chain = try await SSHConnectionChain.connect(target: target, targetAuth: auth, jump: jump)
                self.chain = chain
                guard !isStopped else { await chain.close(); return }
                let shell = try await chain.target.openShell(cols: cols, rows: rows)
                guard !isStopped else { shell.close(); return }
                self.shell = shell
                // Håller anslutningen vaken genom NAT/brandväggars idle-timeout
                // OCH upptäcker att den dött (se SSHShell.startKeepAlive) —
                // stoppas automatiskt av shell.close() i stop().
                //
                // Skriver den gula varningsraden i terminalen SJÄLV innan
                // `shell.close()` river strömmen: går det via `onSessionEnded`
                // har vyn redan hunnit stängas och användaren ser ingenting
                // alls, vilket är precis det tysta försvinnande den här
                // funktionen finns för att få bort.
                shell.startKeepAlive { [weak self] in
                    // Återanropet körs på keep-alive-Task:ens kontext, inte
                    // på main — och hela den här klassen är @MainActor. Utan
                    // hoppet vore varje läsning av `isStopped`/`onData` en
                    // isoleringsöverträdelse.
                    Task { @MainActor in
                        guard let self, !self.isStopped else { return }
                        let notice = "\r\n\u{1b}[33m[bastion] Anslutningen bröts oväntat — "
                            + "servern slutade svara. Den shell som kördes är borta och går "
                            + "inte att återuppta där den slutade. Sessionen måste startas "
                            + "om.\u{1b}[0m\r\n"
                        self.onData?(ArraySlice(Array(notice.utf8)))
                    }
                }
                if let cmd = initialCommand { shell.send(cmd + "\n") }
                for try await chunk in shell.output {
                    guard !isStopped else { break }
                    let bytes = chunk.bytes
                    self.onData?(bytes[...])
                }
                // Strömmen tog slut NORMALT — fjärrshellen stängde (t.ex.
                // `exit`/Ctrl+D). Måste städas här precis som i catch-grenen
                // nedan, annars förblir keepAlive-Task:en och den underliggande
                // anslutningen aktiva utan att någon någonsin river ner dem
                // (CodeRabbit-fynd: den här grenen saknade helt städning,
                // till skillnad från LinuxApp/WindowsApp-motsvarigheterna).
                debugLog("session", "output-strömmen tog slut normalt (fjärrshellen stängde)")
                await teardown()
                // Inte samma sak som `isStopped` (användaren stängde vyn
                // själv) — den vägen ska INTE trigga onSessionEnded,
                // anroparen vet redan att den stänger.
                if !isStopped { self.onSessionEnded?() }
            } catch {
                // Om felet kom EFTER att chain redan var uppsatt (openShell()
                // eller output-strömmen misslyckades, inte själva anslutningen)
                // måste den städas här — SSHConnectionChain.connect() städar
                // bara sina EGNA fel internt, inte fel som inträffar efter att
                // den redan returnerat. Ofarligt no-op om chain fortfarande är
                // nil (connect() self själv redan städat i den vägen).
                await teardown()
                debugLog("session", "fel: \(error)")
                guard !isStopped else { return }
                let msg = Array("\r\n[bastion] fel: \(error)\r\n".utf8)
                self.onData?(msg[...])
                // Utan detta lämnades SessionView öppen med en död session
                // efter ett anslutningsfel — exakt samma "måste trycka Klar
                // manuellt"-problem som exit/Ctrl+D-fixen ovan löste för
                // NORMAL avslutning, bara via felvägen istället
                // (CodeRabbit-fynd). `onSessionEnded` dokumenteras nu även
                // täcka fel, inte bara exit/Ctrl+D — se SessionView.swift.
                self.onSessionEnded?()
            }
        }
    }

    func sendKeys(_ data: ArraySlice<UInt8>) { shell?.send(Array(data)) }
    func resize(cols: Int, rows: Int) { shell?.resize(cols: cols, rows: rows) }
    func stop() {
        isStopped = true
        Task { await teardown() }
    }

    /// Den ENDA platsen som faktiskt stänger `shell`/`chain` — anropad från
    /// `stop()` OCH från båda avslutningsvägarna i `start()`s Task. Klar/
    /// nollställ `shell`/`chain` INNAN någon `await`, så en konkurrerande
    /// anropare (kollar `isTornDown` på samma `@MainActor`, aldrig över en
    /// `await`-punkt) garanterat ser att jobbet redan är taget istället för
    /// att båda stänger samma resurser/startar överlappande
    /// `chain.close()`-anrop (CodeRabbit-fynd).
    private func teardown() async {
        guard !isTornDown else { return }
        isTornDown = true
        let shell = self.shell
        let chain = self.chain
        self.shell = nil
        self.chain = nil
        shell?.close()
        await chain?.close()
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
    /// Se `SSHTerminalController.onSessionEnded`.
    var onSessionEnded: (() -> Void)? = nil

    func makeCoordinator() -> Coordinator {
        Coordinator(target: target, auth: auth, jump: jump, initialCommand: initialCommand, onSessionEnded: onSessionEnded)
    }

    private func build(_ context: Context) -> TerminalView {
        let view = TerminalView()
        view.terminalDelegate = context.coordinator
        context.coordinator.attach(view)
        #if os(iOS)
        // Ersätter SwiftTerms egen rad. Se `TerminalKeyboardBar` för vad
        // den saknar — kort version: Alt, F11–F12, och F-tangenterna
        // försvinner tyst när skärmen är smal.
        //
        // Bytesen går genom KOORDINATORN och inte genom vyns
        // delegatmetod: den senare hade krävt vyn som argument till sig
        // själv, och koordinatorn är ändå den som äger anslutningen.
        view.inputAccessoryView = TerminalKeyboardBar { [weak coordinator = context.coordinator] bytes in
            coordinator?.sendBytes(bytes)
        }
        #endif
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

        init(target: SSHTarget, auth: SSHAuth, jump: (target: SSHTarget, auth: SSHAuth)?, initialCommand: String?, onSessionEnded: (() -> Void)?) {
            self.controller = SSHTerminalController(target: target, auth: auth, jump: jump, initialCommand: initialCommand)
            super.init()
            controller.onData = { [weak self] bytes in
                self?.view?.feed(byteArray: bytes)
            }
            controller.onSessionEnded = onSessionEnded
        }

        func attach(_ view: TerminalView) {
            self.view = view
            let savedID = UserDefaults.standard.string(forKey: TerminalThemeKeys.selectedID)
            view.apply(theme: TerminalTheme.theme(id: savedID))
            // SwiftTerms standard (true) låter fjärrprogram (vim/tmux/htop)
            // som ber om musrapportering kapa pekskärmens svep/scroll —
            // touch-scroll slutar då fungera tills man manuellt slår av det
            // (TestFlight-feedback 2026-07-28: "det där borde alltid funka
            // och inte vara något man togglar av och på"). Lokal scroll/
            // markering ska alltid vinna på touch.
            view.allowMouseReporting = false
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

        /// Samma väg, men för tangentbordsraden — den har ingen
        /// `TerminalView` att skicka med som källa.
        func sendBytes(_ bytes: [UInt8]) {
            controller.sendKeys(ArraySlice(bytes))
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
