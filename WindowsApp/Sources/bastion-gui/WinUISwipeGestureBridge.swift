import WinUI

/// Kopplar en genuin WinUI touch/pekar-svepgest (`ManipulationCompleted`) på
/// ett godtyckligt `FrameworkElement`, för touchscreen-navigering mellan
/// flikar/vyer — Windows-motsvarigheten till LinuxApps
/// `GestureSwipeBridge.swift` (GTK). Till skillnad från GTK-sidan behövs
/// INGEN rå WinRT-ABI-koppling här: swift-winui exponerar
/// `UIElement.manipulationMode`/`.manipulationCompleted` direkt via sin
/// genererade WinRT-projektion (`Microsoft.UI.Xaml.Input`) — verifierat i
/// swift-winuis källa (`UIElement.manipulationCompleted: Event<
/// ManipulationCompletedEventHandler>`, `ManipulationCompletedRoutedEventArgs.
/// velocities: ManipulationVelocities`), inga egna C-bindningar behövs.
///
/// EJ verifierat på riktig touchhårdvara (ingen tillgänglig i den här
/// utvecklingsmiljön, 2026-07-28 — bara en iPhone och en Samsung-TV, ingen
/// Windows-touchskärm). Bara verifierat att koden kompilerar/länkar mot
/// swift-winuis publika API på riktig Windows Server 2025-hårdvara. Be
/// användaren bekräfta på riktig touch-Windows-maskin innan detta räknas
/// som helt klart i praktiken — se ROADMAP.md.
///
/// `ManipulationVelocities.linear` är i PIXLAR/MILLISEKUND (Microsoft-
/// dokumentationen), INTE pixlar/sekund som GTKs `swipe`-signal — tröskeln
/// nedan är skalad därefter (0.2 px/ms ≈ 200 px/s, samma känsla som
/// LinuxApp-tröskeln).
@MainActor
func attachSwipeGesture(
    to element: WinUI.FrameworkElement,
    onSwipe: @escaping (Double, Double) -> Void
) {
    // .translateX räcker för horisontell svep-detektion — vertikal scroll
    // (t.ex. i en ScrollViewer inuti samma yta) ska inte fångas upp av
    // gesten, samma avsikt som GTK-sidans egen abs(velocityX) > abs(
    // velocityY)-filtrering i anropsplatsen.
    element.manipulationMode = .translateX
    element.manipulationCompleted.addHandler { _, args in
        guard let args else { return }
        let v = args.velocities.linear
        onSwipe(Double(v.x), Double(v.y))
    }
}
