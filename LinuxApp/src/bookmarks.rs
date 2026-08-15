//! Bokmärken i en sessions logg.
//!
//! Steg 4 under ROADMAP "Riktmärke: Termius" — det sista som återstod av
//! deras Pro-lista, och det enda av den som bastion inte redan hade
//! gratis. Problemet det löser: en lång körning (en deploy, en
//! paketuppgradering, en `tail -f`) skrollar förbi, och det man ville
//! titta på igen ligger tusen rader upp utan något sätt att hitta
//! tillbaka annat än att skrolla och leta.
//!
//! # Vad ett bokmärke är
//!
//! En rad i terminalens skrollbuffert plus en etikett. Positionen är
//! `gtk::Adjustment`-värdet, alltså den ÖVERSTA SYNLIGA raden när
//! bokmärket sattes — inte markörens rad. Det är den definition som
//! håller: att hoppa tillbaka ska återställa vyn så som den såg ut, och
//! den frågan har ett entydigt svar oavsett var markören råkade stå.
//!
//! # Positionen glider om skrollbufferten svämmar över
//!
//! VTE:s `gtk::Adjustment` är RELATIV till bufferten, inte en absolut
//! radräknare: när gamla rader faller ur numreras det som är kvar om, och
//! en sparad position pekar då på en senare rad än den gjorde. Upptäckt
//! genom att köra appen — ett bokmärke satt vid rad 368 landade på rad
//! 658 efter ytterligare 400 utskrivna rader, vilket är exakt hur många
//! som hunnit falla ur en 512-radersbuffert.
//!
//! Två svar på det, båda behövda:
//!
//! 1. Skrollbufferten höjdes från VTE:s förval (512 rader — futtigt för
//!    en SSH-klient som ska klara en `tail -f` eller en paketuppgradering)
//!    till 100 000. Inom den ramen glider ingenting.
//! 2. Har bufferten ÄNDÅ svämmat över säger listan det rent ut i stället
//!    för att låtsas att positionerna stämmer. Se
//!    [`positions_may_have_drifted`] — frågan gäller terminalen som
//!    helhet, inte det enskilda bokmärket: har en enda rad fallit ur har
//!    allt som pekar in i bufferten glidit lika mycket.
//!
//! Det exakta svaret vore att förankra bokmärket i radens INNEHÅLL, men
//! `vte_terminal_get_text_range_format` kräver att crate-featuren
//! `v0_72` slås på, vilket höjer golvet för systemets libvte och därmed
//! berör paketeringen. Inte värt det för det här.
//!
//! # Varför de inte sparas till disk
//!
//! Ett bokmärke pekar in i en skrollbuffert som bara finns så länge
//! sessionen gör det. Att spara det som överlever det den pekar på vore
//! att lova något som inte går att infria — därför lever listan i
//! rutans widget och dör med den. (Termius sparar sina i molnvalvet, men
//! de sparar då hela loggen med.)

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub id: Uuid,
    pub label: String,
    /// Skrollpositionen bokmärket pekar på.
    pub row: f64,
}

#[derive(Debug, Default)]
pub struct BookmarkList {
    items: Vec<Bookmark>,
}

impl BookmarkList {
    pub fn new() -> Self {
        BookmarkList::default()
    }

    /// Lägger till ett bokmärke och ger tillbaka dess id.
    ///
    /// Listan hålls sorterad på position, inte på när bokmärkena sattes:
    /// den som letar i en logg letar uppifrån och ned, inte i den
    /// ordning hen råkade trycka på knappen.
    pub fn add(&mut self, row: f64, label: String) -> Uuid {
        let id = Uuid::new_v4();
        self.items.push(Bookmark { id, label, row });
        self.items
            .sort_by(|a, b| a.row.partial_cmp(&b.row).unwrap_or(std::cmp::Ordering::Equal));
        id
    }

    pub fn remove(&mut self, id: Uuid) -> bool {
        let before = self.items.len();
        self.items.retain(|bookmark| bookmark.id != id);
        self.items.len() != before
    }

    /// Byter etikett. Falskt om bokmärket inte finns.
    pub fn rename(&mut self, id: Uuid, label: String) -> bool {
        match self.items.iter_mut().find(|bookmark| bookmark.id == id) {
            Some(bookmark) => {
                bookmark.label = label;
                true
            }
            None => false,
        }
    }

    pub fn all(&self) -> &[Bookmark] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Etiketten ett bokmärke får när användaren inte skrivit någon.
///
/// Klockslaget, inte "Bokmärke 3": det man minns efteråt är NÄR något
/// hände ("det var strax innan bygget dog"), inte i vilken ordning man
/// tryckte. Tiden kommer in som delar i stället för som en
/// `DateTime`-typ, så att formateringen går att testa utan att testet
/// beror på maskinens tidszon.
pub fn default_label(hour: u32, minute: u32, second: u32) -> String {
    format!("{hour:02}:{minute:02}:{second:02}")
}

/// Kan bokmärkenas positioner ha glidit? Se modulkommentaren.
///
/// `rows_in_buffer` är adjustmentets `upper`, `capacity` skrollbuffertens
/// storlek plus de synliga raderna. Jämförelsen är `>=` och inte `>`:
/// bufferten slutar växa NÄR den är full, och från och med då kastas en
/// rad i toppen för varje ny rad i botten. Är den precis full har alltså
/// den första raden redan hunnit falla ur.
pub fn positions_may_have_drifted(rows_in_buffer: f64, capacity: f64) -> bool {
    rows_in_buffer >= capacity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_label_is_a_zero_padded_clock() {
        assert_eq!(default_label(9, 5, 3), "09:05:03");
        assert_eq!(default_label(23, 59, 59), "23:59:59");
        assert_eq!(default_label(0, 0, 0), "00:00:00");
    }

    /// Ordningen i listan ska följa LOGGEN, inte klickordningen — den som
    /// bokmärker något långt upp efteråt ska hitta det ovanför det som
    /// redan låg längre ned.
    #[test]
    fn bookmarks_are_listed_in_log_order_not_insertion_order() {
        let mut list = BookmarkList::new();
        list.add(900.0, "sist i loggen".into());
        list.add(120.0, "tidigt".into());
        list.add(450.0, "mitten".into());

        let labels: Vec<&str> = list.all().iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, vec!["tidigt", "mitten", "sist i loggen"]);
    }

    #[test]
    fn two_bookmarks_on_the_same_row_both_survive() {
        let mut list = BookmarkList::new();
        let first = list.add(42.0, "ett".into());
        let second = list.add(42.0, "två".into());
        assert_ne!(first, second, "varje bokmärke ska ha eget id");
        assert_eq!(list.all().len(), 2);
    }

    #[test]
    fn rename_and_remove_only_touch_the_named_bookmark() {
        let mut list = BookmarkList::new();
        let first = list.add(10.0, "ett".into());
        let second = list.add(20.0, "två".into());

        assert!(list.rename(first, "döpt".into()));
        assert!(!list.rename(Uuid::new_v4(), "finns inte".into()));
        assert_eq!(list.all()[0].label, "döpt");
        assert_eq!(list.all()[1].label, "två", "grannen ska vara orörd");

        assert!(list.remove(second));
        assert!(!list.remove(second), "att ta bort samma två gånger är ingen ändring");
        assert_eq!(list.all().len(), 1);
        assert_eq!(list.all()[0].id, first);
    }

    /// Gränsfallet är hela poängen: en buffert som är PRECIS full har
    /// redan tappat sin första rad.
    #[test]
    fn drift_starts_exactly_when_the_buffer_becomes_full() {
        assert!(!positions_may_have_drifted(511.0, 512.0));
        assert!(positions_may_have_drifted(512.0, 512.0));
        assert!(positions_may_have_drifted(900.0, 512.0));
        assert!(
            !positions_may_have_drifted(0.0, 100_000.0),
            "en tom buffert har inte tappat något"
        );
    }

    /// En `NaN`-position får inte få sorteringen att panika. `f64`
    /// saknar total ordning, och `sort_by` med en komparator som bryter
    /// mot sina egna regler är ett dokumenterat panikfall i std.
    #[test]
    fn a_nan_position_does_not_panic_the_sort() {
        let mut list = BookmarkList::new();
        list.add(10.0, "ett".into());
        list.add(f64::NAN, "trasig".into());
        list.add(5.0, "två".into());
        assert_eq!(list.all().len(), 3);
    }
}
