import CGtk4Raw
import Gtk

/// Riktig, radlös tangentbordsinmatning för Linux-terminalen — svep-gestens
/// syskon (se GestureSwipeBridge.swift, samma tekniker återanvänds här).
///
/// `key-pressed` på `GtkEventControllerKey` returnerar `gboolean`
/// (`TRUE` = hanterat, stoppar vidare spridning av samma tangenttryck —
/// `_gtk_boolean_handled_accumulator` i GTK:s egen källkod). swift-cross-uis
/// kodgenererade `Gtk.EventControllerKey`-wrapper (`Sources/Gtk/Generated/
/// EventControllerKey.swift`) registrerar signalen med en `Void`-returnerande
/// C-trampolin — en ABI-diskrepans (CodeRabbit-fynd, den här PR:n): GTK:s
/// marskalkning läser då ett odefinierat värde ur returregistret i stället
/// för ett riktigt `TRUE`/`FALSE`, vilket kan göra att t.ex. piltangenter
/// BÅDE skickas som escape-sekvenser HÄR och rullar `ScrollView`n via GTK:s
/// egen standardhantering. Löst genom att koppla signalen rakt av via GLibs
/// publika `g_signal_connect_data` (samma mönster som `GestureSwipeBridge.
/// swift` redan använder för `GtkGestureSwipe`, som saknar en wrapper helt)
/// med en EGEN, korrekt `gboolean`-returnerande trampolin i stället för att
/// gå via den trasiga wrappern.
private final class KeyPressBox {
    let onKey: (String) -> Void
    init(onKey: @escaping (String) -> Void) { self.onKey = onKey }
}

/// GDK-modifierflaggor (gdkenums.h, `GdkModifierType`) som filtrerar bort
/// tangenttryck HELT — Ctrl/Alt/Super/Hyper/Meta-kombinationer ska aldrig
/// tolkas som text (CodeRabbit-fynd: utan detta skulle t.ex. fysisk Ctrl+C
/// skicka en bokstavlig "c" till PTY:n, inte ingenting — den befintliga
/// Ctrl+C-knappen är den avsedda vägen för det). Skift ingår MEDVETET inte:
/// GTK levererar redan den SKIFTADE keyvalen för Skift+bokstav (t.ex. 'A' i
/// stället för 'a' + ett skift-flagg), `gdk_keyval_to_unicode` hanterar det
/// utan någon egen tolkning här.
private let ignoredModifierMask: UInt32 =
    (1 << 2)  // GDK_CONTROL_MASK
    | (1 << 3)  // GDK_ALT_MASK
    | (1 << 26)  // GDK_SUPER_MASK
    | (1 << 27)  // GDK_HYPER_MASK
    | (1 << 28)  // GDK_META_MASK

/// Matchar `key-pressed`s riktiga C-signatur (`gboolean (*)(
/// GtkEventControllerKey*, guint keyval, guint keycode, GdkModifierType
/// state, gpointer)`) — `guint`/`GdkModifierType` är båda 4 byte på alla
/// plattformar GTK4 stödjer, därav `UInt32` rakt av i stället för att bero
/// på exakt vilken Swift-typ `GdkModifierType` råkar importeras som (osäkert
/// utan en kompilator till hands, se README/PR-beskrivningen för varför).
private let keyPressedHandler: @convention(c) (
    UnsafeMutableRawPointer?, UInt32, UInt32, UInt32, UnsafeMutableRawPointer?
) -> Int32 = { _, keyval, _, state, data in
    guard let data else { return 0 }
    guard state & ignoredModifierMask == 0, let text = translateKeyval(UInt(keyval)) else {
        return 0  // FALSE — inte hanterat här, GTK:s normala hantering fortsätter.
    }
    Unmanaged<KeyPressBox>.fromOpaque(data).takeUnretainedValue().onKey(text)
    return 1  // TRUE — hanterat, stoppar vidare spridning av samma tangenttryck.
}

/// Körs av GLib när signalen kopplas bort / kontrollen förstörs — släpper
/// den starkt hållna `KeyPressBox` så den inte läcker för varje flikbyte.
private let keyPressNotifyHandler: @convention(c) (
    UnsafeMutableRawPointer?, UnsafeMutablePointer<GClosure>?
) -> Void = { data, _ in
    guard let data else { return }
    Unmanaged<KeyPressBox>.fromOpaque(data).release()
}

/// Ansluter riktig tangentbordsinmatning till `widget`. `onKey` får rå text
/// (escape-sekvenser för navigeringstangenter, annars det skrivbara tecknet)
/// att skicka rakt till PTY:n.
@MainActor
func attachKeyCapture(to widget: Gtk.Widget, onKey: @escaping (String) -> Void) {
    // widgetOpaquePointer kollas FÖRST, innan kontrollen skapas — samma
    // läcko-skydd som GestureSwipeBridge.swift (CodeRabbit-fynd, PR #215):
    // om widget.opaquePointer är nil skulle gtk_widget_add_controller
    // (kontrollens enda ägare) aldrig nås, och dess destroy-notify (som
    // annars släpper boxen) fyras aldrig.
    guard let widgetOpaquePointer = widget.opaquePointer,
          let controllerPointer = gtk_event_controller_key_new()
    else { return }

    let box = KeyPressBox(onKey: onKey)
    let boxPointer: UnsafeMutableRawPointer = Unmanaged.passRetained(box).toOpaque()
    let instancePointer: UnsafeMutableRawPointer = UnsafeMutableRawPointer(controllerPointer)
    let signalCallback: GCallback = unsafeBitCast(keyPressedHandler, to: GCallback.self)
    let flags = GConnectFlags(rawValue: 0)

    _ = CGtk4Raw.g_signal_connect_data(
        instancePointer,
        "key-pressed",
        signalCallback,
        boxPointer,
        keyPressNotifyHandler,
        flags
    )

    let widgetPointer = UnsafeMutablePointer<GtkWidget>(widgetOpaquePointer)
    gtk_widget_add_controller(widgetPointer, controllerPointer)

    // gtk_widget_set_can_focus/set_focusable/grab_focus saknar publika
    // Swift-wrappers i swift-cross-uis Gtk-paket, därav CGtk4Raw även här.
    gtk_widget_set_can_focus(widgetPointer, 1)
    gtk_widget_set_focusable(widgetPointer, 1)

    // Klick ger widgeten fokus (den är ingen naturligt interaktiv widget som
    // en knapp/textruta, så GTK gör det inte automatiskt bara för att den är
    // fokuserbar) — samma `OpaquePointer`->`UnsafeMutablePointer<GtkWidget>`-
    // brygga som ovan, återanvänd i varje klick.
    let click = Gtk.GestureClick()
    click.pressed = { [weak widget] _, _, _, _ in
        guard let pointer = widget?.opaquePointer else { return }
        _ = gtk_widget_grab_focus(UnsafeMutablePointer<GtkWidget>(pointer))
    }
    widget.addEventController(click)

    // Fokus direkt vid uppstart också — en nyöppnad terminalflik ska gå att
    // skriva i utan att först behöva klicka.
    _ = gtk_widget_grab_focus(widgetPointer)
}

/// Översätter en GDK-keyval till rå bytes att skicka till PTY:n, eller `nil`
/// om tangenten inte hanteras (t.ex. rena funktionstangenter F1-F12).
/// `gdk_keyval_to_unicode` täcker versaler/gemener och alla tecken utanför
/// ASCII (GTK levererar redan den SKIFTADE keyvalen för Skift+bokstav, ingen
/// egen skift-hantering behövs här).
private func translateKeyval(_ keyval: UInt) -> String? {
    switch keyval {
    case UInt(GDK_KEY_Left): return "\u{1B}[D"
    case UInt(GDK_KEY_Right): return "\u{1B}[C"
    case UInt(GDK_KEY_Up): return "\u{1B}[A"
    case UInt(GDK_KEY_Down): return "\u{1B}[B"
    case UInt(GDK_KEY_Home): return "\u{1B}[H"
    case UInt(GDK_KEY_End): return "\u{1B}[F"
    case UInt(GDK_KEY_Page_Up): return "\u{1B}[5~"
    case UInt(GDK_KEY_Page_Down): return "\u{1B}[6~"
    case UInt(GDK_KEY_Delete): return "\u{1B}[3~"
    case UInt(GDK_KEY_BackSpace): return "\u{7F}"
    case UInt(GDK_KEY_Tab): return "\t"
    case UInt(GDK_KEY_Return): return "\r"
    case UInt(GDK_KEY_Escape): return "\u{1B}"
    default:
        let unicode = gdk_keyval_to_unicode(UInt32(keyval))
        // Utesluter C0-styrtecken (0x00-0x1F) och DEL (0x7F) — bara riktigt
        // skrivbara tecken (ASCII och utökat Unicode) släpps igenom. Rena
        // funktionstangenter (F1-F12, Insert m.fl.) ger 0 här och ignoreras.
        guard unicode >= 0x20, unicode != 0x7F, let scalar = Unicode.Scalar(unicode) else {
            return nil
        }
        return String(Character(scalar))
    }
}
