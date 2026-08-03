import SwiftCrossUI
import WinUIBackend

/// Windows-motsvarigheten till `LinuxApp/` — samma SSHCore, samma
/// host-databas, samma vyer (`ContentView.swift` m.fl., kopierade rakt av
/// från `LinuxApp/Sources/bastion-gui/` — ren SwiftCrossUI-kod, inga
/// GTK-specifika API:er), bara `WinUIBackend` istället för `GtkBackend`.
/// Två filer portades INTE: `BastionGUIApp.swift` (den här, eget @main per
/// paket) och `GestureSwipeBridge.swift` (rå GLib/GTK4-signalkoppling för
/// touchscreen-svep — se `TerminalTabsView.swift`s kommentar för varför).
/// Ingen lokal Windows-maskin att testköra mot ännu, så varje steg
/// verifieras via CI (`windows-gui.yml`) istället för lokalt (som för `App/`).
@main
struct BastionGUIApp: App {
    var body: some Scene {
        WindowGroup("Bastion") {
            ContentView()
        }
        .defaultSize(width: 900, height: 560)
    }
}
