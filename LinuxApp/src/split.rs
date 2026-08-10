//! Delad vy — flera terminaler sida vid sida i EN flik.
//!
//! Steg 2 i Termius-jämförelsen (se ROADMAP "Riktmärke: Termius"): appen
//! hade flikar men ingen delad vy, så två maskiner samtidigt betydde att
//! byta flik fram och tillbaka. Deras Split View går upp till 16 rutor;
//! här sätts ingen övre gräns — trädet är rekursivt, det är fönstrets
//! storlek som blir gränsen i praktiken.
//!
//! # Varför ett träd av `gtk::Paned` och inte ett rutnät
//!
//! `gtk::Paned` tar exakt två barn och har en drag­bar avdelare gratis.
//! En ruta som delas byts ut mot en ny `Paned` med sig själv i ena halvan
//! och den nya rutan i den andra. Det ger godtyckliga layouter (dela
//! höger, sedan dela den nedre halvan, …) utan någon egen layoutkod, och
//! det är samma modell som tmux och Terminator använder.
//!
//! # Varför varje flik har en `pane_root`
//!
//! `AdwTabPage:child` är CONSTRUCT_ONLY — en sidas barn går inte att byta
//! ut efteråt. En flik vars barn är terminalen SJÄLV går alltså aldrig att
//! dela. Därför får varje terminalflik en tom `gtk::Box` som barn, och
//! terminalen (eller `Paned`-trädet) bor i den. Boxen är rotnoden som
//! `replace_child` kan skriva i.
//!
//! # Varför widgetträdet är den enda sanningen
//!
//! Ingen parallell layoutmodell hålls vid sidan om. GTK vet redan exakt
//! hur rutorna sitter; en kopia av samma information hade bara kunnat
//! hamna ur synk. Priset är att modulen är ren GTK-kod och därmed inte
//! enhetstestbar utan en display (samma villkor som `main.rs` själv, se
//! dess kommentar om testtäckning) — den verifieras genom att appen körs
//! under Xvfb.

use gtk::prelude::*;

/// Vad som hände med fliken när en ruta stängdes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneClosed {
    /// Rutan togs bort och syskonrutan tog över dess plats — fliken lever
    /// vidare med minst en ruta kvar.
    PanesRemain,
    /// Det var flikens enda ruta. Ingenting har rörts i widgetträdet:
    /// anroparen ska stänga hela fliken i stället.
    PageEmpty,
    /// Rutan satt inte i något ruttträd — den är redan borttagen, och
    /// anroparen ska inte göra någonting alls.
    ///
    /// Fallet är inte teoretiskt: varje ruta stängs TVÅ gånger. Först när
    /// användaren stänger den, sedan en gång till när bakgrundstråden
    /// märker att kanalen stängts och skickar sitt `Closed`. Utan det här
    /// svaret läste den andra omgången "ingen `Paned` ovanför mig" som
    /// "jag är flikens sista ruta" och stängde HELA FLIKEN med alla dess
    /// andra sessioner. Reproducerat med tre rutor: en `Ctrl+Shift+X`
    /// tog alla tre.
    AlreadyGone,
}

/// Rotnoden för en flik som ska gå att dela. Se modulkommentaren om
/// `AdwTabPage:child` för varför den behövs även när fliken bara har en
/// ruta.
pub fn pane_root() -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, 0)
}

/// Alla terminaler i ett widgetträd, i den ordning de sitter (vänster före
/// höger, övre före nedre — `Paned` lägger sitt start-barn först).
///
/// Städning och temabyten gällde tidigare `page.child()` direkt, vilket
/// var samma sak som terminalen så länge en flik var en ruta. Med delad
/// vy måste båda i stället gälla varje ruta i fliken.
pub fn terminals_in(root: &impl IsA<gtk::Widget>) -> Vec<vte::Terminal> {
    let mut found = Vec::new();
    collect_terminals(root.as_ref(), &mut found);
    found
}

fn collect_terminals(widget: &gtk::Widget, found: &mut Vec<vte::Terminal>) {
    if let Some(terminal) = widget.downcast_ref::<vte::Terminal>() {
        found.push(terminal.clone());
        return;
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        // Syskonet hämtas INNAN rekursionen. Anropen här läser bara
        // trädet, men ordningen gör det omöjligt att av misstag följa en
        // länk i ett träd som just ändrats.
        child = current.next_sibling();
        collect_terminals(&current, found);
    }
}

/// Terminalen som tar emot tangenttryckningar just nu, eller den första
/// rutan om ingen har fokus (fliken kan ha valts med tangentbordet utan
/// att någon terminal hunnit få fokus).
///
/// Frågan ställs till FÖNSTRET (`gtk::Window::focus`), inte till varje
/// widget med `has_focus`. Det senare är "har den globala inmatnings-
/// fokusen" och kräver att fönstret dessutom är aktivt — kör man appen
/// utan fönsterhanterare (Xvfb, som i verifieringen av just den här
/// funktionen) är det aldrig sant, och DÅ delades alltid den FÖRSTA
/// rutan i stället för den man skrev i. Fönstrets fokuswidget är sann
/// oavsett, och den kan dessutom vara ett barn till terminalen, så
/// sökningen går uppåt tills en ruta i det här trädet hittas.
pub fn focused_terminal(root: &impl IsA<gtk::Widget>) -> Option<vte::Terminal> {
    let root = root.as_ref();
    let terminals = terminals_in(root);

    // `focus` finns både på `GtkWindow` (fokuswidgeten) och på `GtkRoot`
    // (samma sak, ärvd) — traiten måste pekas ut.
    let focus: Option<gtk::Widget> = root
        .root()
        .and_downcast::<gtk::Window>()
        .and_then(|window| gtk::prelude::GtkWindowExt::focus(&window));
    let mut candidate = focus;
    while let Some(widget) = candidate {
        if let Some(terminal) = widget
            .downcast_ref::<vte::Terminal>()
            .filter(|terminal| terminals.contains(terminal))
        {
            return Some(terminal.clone());
        }
        candidate = widget.parent();
    }

    terminals.first().cloned()
}

/// Delar `pane` och sätter `new_pane` i den nya halvan.
///
/// `orientation` är avdelarens riktning på GTK:s vis: `Horizontal` lägger
/// rutorna sida vid sida (den nya till höger), `Vertical` lägger dem över
/// varandra (den nya nedanför).
pub fn split(
    pane: &impl IsA<gtk::Widget>,
    orientation: gtk::Orientation,
    new_pane: &impl IsA<gtk::Widget>,
) {
    let pane = pane.as_ref();
    let Some(parent) = pane.parent() else {
        // Rutan sitter inte i något träd — inget att dela. Kan hända om
        // sessionen hann stängas mellan att åtgärden aktiverades och att
        // den kördes.
        return;
    };

    let paned = gtk::Paned::builder()
        .orientation(orientation)
        .resize_start_child(true)
        .resize_end_child(true)
        // Utan detta går en ruta att dra ihop till noll bredd, och då är
        // den omöjlig att få tillbaka med musen.
        .shrink_start_child(false)
        .shrink_end_child(false)
        .build();

    // `replace_child` kopplar loss `pane` från sin förälder — den måste
    // vara föräldralös innan den kan sättas in i `paned`.
    replace_child(&parent, pane, &paned);
    paned.set_start_child(Some(pane));
    paned.set_end_child(Some(new_pane.as_ref()));
}

/// Tar bort `pane` ur flikens ruttträd och låter syskonrutan ta över
/// platsen. Se [`PaneClosed`] för fallet då rutan var flikens sista.
pub fn close_pane(pane: &impl IsA<gtk::Widget>) -> PaneClosed {
    let pane = pane.as_ref();
    let Some(parent) = pane.parent() else {
        return PaneClosed::AlreadyGone;
    };
    let Some(paned) = parent.downcast_ref::<gtk::Paned>() else {
        // Föräldern är `pane_root`: flikens sista ruta.
        return PaneClosed::PageEmpty;
    };

    let sibling = if paned.start_child().as_ref() == Some(pane) {
        paned.end_child()
    } else {
        paned.start_child()
    };

    let survivor_terminal = sibling.as_ref().and_then(|s| terminals_in(s).first().cloned());

    // Fönstrets fokus nollas FÖRE bortkopplingen. Vilken av `Paned`ens två
    // halvor som än kopplas loss är en av dem dess fokusbarn, och GTK
    // klagar när fokusbarnet försvinner under den ("Error finding last
    // focus widget of GtkPaned … was called on widget (nil)") — ett
    // klagomål per stängd ruta. Fokus sätts på syskonet igen efteråt.
    if let Some(window) = pane.root().and_downcast::<gtk::Window>() {
        gtk::prelude::GtkWindowExt::set_focus(&window, gtk::Widget::NONE);
    }

    // Båda barnen kopplas loss innan `paned` byts ut, annars försöker
    // syskonet sättas in på ett nytt ställe medan det fortfarande har en
    // förälder (GTK-CRITICAL).
    paned.set_start_child(gtk::Widget::NONE);
    paned.set_end_child(gtk::Widget::NONE);

    if let (Some(grandparent), Some(sibling)) = (paned.parent(), sibling.as_ref()) {
        replace_child(&grandparent, paned.upcast_ref(), sibling);
        // Fokus försvinner annars ut ur fliken helt när den ruta som hade
        // det togs bort, och tangenttryckningar hamnar ingenstans.
        if let Some(terminal) = &survivor_terminal {
            terminal.grab_focus();
        }
    }

    PaneClosed::PanesRemain
}

/// Sätter `new` där `old` satt i `parent`. Hanterar de två behållartyper
/// ett ruttträd består av; `old` blir föräldralös.
fn replace_child(parent: &gtk::Widget, old: &gtk::Widget, new: &impl IsA<gtk::Widget>) {
    if let Some(paned) = parent.downcast_ref::<gtk::Paned>() {
        if paned.start_child().as_ref() == Some(old) {
            paned.set_start_child(gtk::Widget::NONE);
            paned.set_start_child(Some(new.as_ref()));
        } else {
            paned.set_end_child(gtk::Widget::NONE);
            paned.set_end_child(Some(new.as_ref()));
        }
    } else if let Some(root) = parent.downcast_ref::<gtk::Box>() {
        root.remove(old);
        root.append(new.as_ref());
    }
}
