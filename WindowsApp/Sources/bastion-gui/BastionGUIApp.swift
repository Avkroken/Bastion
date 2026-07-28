import SSHCore
import SwiftCrossUI
import WinUI
import WinUIBackend

/// Windows-motsvarigheten till `LinuxApp/` — samma SSHCore, samma
/// host-databas, men SwiftCrossUIs `WinUIBackend` istället för `GtkBackend`.
/// Medvetet minimal första version: bevisar att pipelinen (Package.swift +
/// CI på windows-latest-runnern) faktiskt kompilerar innan de riktiga
/// vyerna i `LinuxApp/Sources/bastion-gui/` porteras hit. Ingen lokal
/// Windows-maskin att testköra mot ännu, så varje steg görs litet och
/// verifieras via CI istället för lokalt (som för `App/`).
@main
struct BastionGUIApp: App {
    var body: some Scene {
        WindowGroup("Bastion") {
            ContentView()
        }
        .defaultSize(width: 900, height: 560)
    }
}

struct ContentView: View {
    private var hostCount: Int {
        HostStore().all().count
    }

    // Två platshållarsidor bara för att bevisa svep-navigering fungerar
    // end-to-end INNAN de riktiga vyerna (host-lista, terminalflikar) från
    // LinuxApp/Sources/bastion-gui/ porteras hit — samma anledning till att
    // ContentView i övrigt fortfarande är en platshållare, se ROADMAP.md.
    @State private var page = 0

    var body: some View {
        VStack(spacing: 12) {
            if page == 0 {
                Text("Bastion för Windows").font(.title2)
                Text("\(hostCount) sparade värdar").foregroundColor(.gray)
            } else {
                Text("Inställningar").font(.title2)
                Text("Platshållare — svep tillbaka åt vänster.").foregroundColor(.gray)
            }
            Text("Fullständigt UI porteras hit i ett senare steg.").foregroundColor(.gray)
            Text("Svep vänster/höger för att växla sida (touch, EJ verifierat på riktig hårdvara).")
                .font(.caption).foregroundColor(.gray)
        }
        .padding()
        // Se WinUISwipeGestureBridge.swift — samma velocity-tröskel-mönster
        // (dominerande horisontell rörelse, > 0.2 px/ms) som LinuxApps
        // GestureSwipeBridge-anropsplats i TerminalTabsView.swift.
        .inspect(.onCreate) { [self] (element: WinUI.Canvas) in
            attachSwipeGesture(to: element) { velocityX, velocityY in
                guard abs(velocityX) > abs(velocityY), abs(velocityX) > 0.2 else { return }
                page = velocityX < 0 ? 1 : 0
            }
        }
    }
}
