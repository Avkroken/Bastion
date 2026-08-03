import CGtk4Raw
import Gtk

/// Riktig, radlös tangentbordsinmatning för Linux-terminalen — svep-gestens
/// syskon (se GestureSwipeBridge.swift). Tidigare ansågs det här kräva att gå
/// under SwiftCrossUI direkt mot GTK:s event-controllers (se ROADMAP.md
/// "Uppskjutet med avsikt") — det stämmer bara delvis: swift-cross-uis EGET
/// `Gtk`-paket har redan en publik, kodgenererad wrapper för
/// `GtkEventControllerKey` (`Sources/Gtk/Generated/EventControllerKey.swift`,
/// samma mönster som `Window.setEscapeKeyPressedHandler` i deras egen
/// källkod redan använder för att stänga fönster på Escape) — ingen rå
/// GLib-signalkoppling behövs för själva tangenttrycket, bara för att GÖRA
/// widgeten fokuserbar och ge den fokus vid klick (`gtk_widget_set_
/// can_focus`/`set_focusable`/`grab_focus` saknar publika Swift-wrappers,
/// därav `CGtk4Raw` här — samma `OpaquePointer`-brygga som
/// GestureSwipeBridge.swift redan bevisat säker).
///
/// Avsiktligt AVGRÄNSAT till vanlig text + navigeringstangenter (piltangenter/
/// Tab/Esc/Backspace/Delete/Enter/Home/End/PgUp/PgDn) — INTE Ctrl-kombinationer
/// (Ctrl+C m.fl.), som redan täcks av de befintliga knapparna. Att tolka
/// Ctrl-kombinationer korrekt kräver `GdkModifierType` (bitflagg-facken i
/// `keyPressed`s tredje parameter), en typ vars exakta Swift-namnbryggning
/// (bara delvis verifierad utan en kompilator till hands) hölls medvetet
/// utanför den här första versionen — mindre riskabelt att skicka utan.
@MainActor
func attachKeyCapture(to widget: Gtk.Widget, onKey: @escaping (String) -> Void) {
    let controller = Gtk.EventControllerKey()
    controller.keyPressed = { _, keyval, _, _ in
        guard let text = translateKeyval(keyval) else { return }
        onKey(text)
    }
    widget.addEventController(controller)

    guard let widgetOpaquePointer = widget.opaquePointer else { return }
    let widgetPointer = UnsafeMutablePointer<GtkWidget>(widgetOpaquePointer)
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
