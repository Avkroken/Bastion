import SSHCore
import SwiftCrossUI
import WinUI
import WinUIBackend

/// Windows-motsvarigheten till `LinuxApp/` — samma SSHCore, samma
/// host-databas, samma vyer (portade från `LinuxApp/Sources/bastion-gui/`,
/// se ROADMAP.md för portningshistoriken), men SwiftCrossUIs `WinUIBackend`
/// istället för `GtkBackend`. De tre filer som skiljer sig mellan
/// plattformarna (denna, `TerminalTabsView.swift`, `WinUISwipeGestureBridge.swift`
/// vs LinuxApps `GestureSwipeBridge.swift`) är just de som rör den råa
/// touch-gest-kopplingen — allt annat är oförändrad SwiftCrossUI-kod.
@main
struct BastionGUIApp: App {
    var body: some Scene {
        WindowGroup("Bastion") {
            ContentView()
        }
        .defaultSize(width: 900, height: 560)
    }
}
