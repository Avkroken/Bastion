import CGtk4Raw
import Gtk

/// Kopplar en riktig GTK4-svep-gest (`GtkGestureSwipe`) på en godtycklig
/// widget, för touchscreen-navigering mellan flikar/vyer.
///
/// swift-cross-uis egna `Gtk`-paket har INTE kodgenererat en Swift-wrapper
/// för `GtkGestureSwipe` (bara Click/LongPress finns i
/// `Sources/Gtk/Generated`), och deras interna signal-koppling
/// (`GObject.addSignal`) är `internal` — inte synlig utanför deras eget
/// modulgränssnitt. Den här filen kopplar därför signalen direkt via GLibs
/// publika `g_signal_connect_data`, via en EGEN lokal `CGtk4Raw`-
/// systembiblioteksmodul (samma `pkg-config gtk4` som deras `CGtk`, men en
/// egen Swift-importerad kopia eftersom `CGtk` inte är en extern produkt).
///
/// `OpaquePointer` är avsiktligt modul-agnostisk i Swift — två separata
/// clang-importerade moduler som pekar på SAMMA underliggande C-typ
/// (`GtkWidget`/`GtkGesture`, båda GObject-opaka för Swifts importerare)
/// producerar kompatibla `OpaquePointer`-värden. `Gtk.Widget.opaquePointer`
/// (från swift-cross-uis `Gtk`-paket) kan alltså skickas rakt in i
/// `CGtk4Raw`s C-funktioner, och `CGtk4Raw.gtk_gesture_swipe_new()`s
/// retur-`OpaquePointer` kan skickas rakt in i `Gtk.EventController`s
/// publika `init(_ pointer: OpaquePointer)` — ingen osäker pekar-omtolkning
/// av innehåll, bara vidarebefordran av redan-opaka referenser.
private final class SwipeGestureBox {
    let onSwipe: (Double, Double) -> Void
    init(onSwipe: @escaping (Double, Double) -> Void) { self.onSwipe = onSwipe }
}

private let swipeSignalHandler: @convention(c) (
    UnsafeMutableRawPointer?, Double, Double, UnsafeMutableRawPointer?
) -> Void = { _, velocityX, velocityY, data in
    guard let data else { return }
    Unmanaged<SwipeGestureBox>.fromOpaque(data).takeUnretainedValue().onSwipe(velocityX, velocityY)
}

/// Körs av GLib när signalen kopplas bort / gesten förstörs — släpper den
/// starkt hållna `SwipeGestureBox` så den inte läcker för varje flikbyte.
private let swipeNotifyHandler: @convention(c) (
    UnsafeMutableRawPointer?, UnsafeMutablePointer<GClosure>?
) -> Void = { data, _ in
    guard let data else { return }
    Unmanaged<SwipeGestureBox>.fromOpaque(data).release()
}

/// Ansluter en svep-gest till `widget`. `onSwipe` får riktningens hastighet
/// (px/s) — negativt `velocityX` = svep åt vänster, positivt = åt höger.
/// Endast horisontell rörelse är relevant för flikbyte, men båda axlarna
/// skickas vidare ifall en framtida vy vill skilja på vertikalt svep.
@MainActor
func attachSwipeGesture(to widget: Gtk.Widget, onSwipe: @escaping (Double, Double) -> Void) {
    guard let gesturePointer = gtk_gesture_swipe_new() else { return }
    let box = SwipeGestureBox(onSwipe: onSwipe)
    let boxPointer: UnsafeMutableRawPointer = Unmanaged.passRetained(box).toOpaque()
    let instancePointer: UnsafeMutableRawPointer = UnsafeMutableRawPointer(gesturePointer)
    let signalCallback: GCallback = unsafeBitCast(swipeSignalHandler, to: GCallback.self)
    let flags = GConnectFlags(rawValue: 0)

    _ = CGtk4Raw.g_signal_connect_data(
        instancePointer,
        "swipe",
        signalCallback,
        boxPointer,
        swipeNotifyHandler,
        flags
    )

    guard let widgetOpaquePointer = widget.opaquePointer else { return }
    // `gtk_widget_add_controller`s widget-parameter importeras som en
    // KONKRET `UnsafeMutablePointer<GtkWidget>` (till skillnad från
    // gest-parametern, som förblir `OpaquePointer` — GtkWidget har synliga
    // structfält, GtkGesture inte) — bygg om via den generiska
    // OpaquePointer-bryggan.
    let widgetPointer = UnsafeMutablePointer<GtkWidget>(widgetOpaquePointer)
    gtk_widget_add_controller(widgetPointer, gesturePointer)
}
