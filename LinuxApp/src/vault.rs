//! Valvet — kategorierna för allt appen har SPARAT, till skillnad från
//! det den kör just nu.
//!
//! Steg 3 under ROADMAP "Riktmärke: Termius". Deras uttalade skäl till
//! att samla sparad data på ett ställe: "each new element increased
//! navigation complexity". Bastion hade precis det problemet — värdarna
//! bodde i sidopanelen medan WireGuard-profiler och S3-anslutningar låg
//! i var sitt dialogfönster bakom primärmenyn, och `known_hosts` hade
//! ingen yta alls (felmeddelandet vid en ändrad värdnyckel bad rent ut
//! användaren att redigera filen för hand). Samma sorts innehåll, tre
//! olika sätt att nå det.
//!
//! Kategorierna beskrivs här som ren data av samma skäl som
//! `palette_actions` gör det: listan blir testbar, vilket `main.rs` inte
//! är (ingen `#[cfg(test)]`-täckning av hävd).

/// En kategori i valvet.
pub struct VaultCategory {
    /// Namnet på `gtk::Stack`-sidan i `main.rs`. Ett id som inte matchar
    /// en sida ger en TOM sidopanel utan minsta felmeddelande — därav
    /// testet längst ned som läser `main.rs`.
    pub id: &'static str,
    /// Det som står i väljaren.
    pub label: &'static str,
    /// Vad `+`-knappen gör när kategorin är vald, eller `None` för en
    /// kategori man inte lägger till i för hand — knappen göms då. Kända
    /// värdar är det fallet: rader hamnar där genom att man ansluter,
    /// aldrig genom att någon skriver in en nyckel.
    pub add: Option<VaultAdd>,
}

/// `+`-knappens innebörd i en kategori.
pub struct VaultAdd {
    pub tooltip: &'static str,
    /// Fullständigt åtgärdsnamn, `app.`-prefixet inkluderat — formen
    /// `gtk::Actionable::set_action_name` vill ha.
    pub action: &'static str,
}

/// Ordningen de visas i: det man rör oftast först. Värdar är förvalet.
pub const CATEGORIES: &[VaultCategory] = &[
    VaultCategory {
        id: "hosts",
        label: "Värdar",
        add: Some(VaultAdd {
            tooltip: "Lägg till värd",
            action: "app.new-host",
        }),
    },
    VaultCategory {
        id: "wireguard",
        label: "WireGuard-profiler",
        add: Some(VaultAdd {
            tooltip: "Ny WireGuard-profil",
            action: "app.new-wireguard",
        }),
    },
    VaultCategory {
        id: "s3",
        label: "S3-anslutningar",
        add: Some(VaultAdd {
            tooltip: "Ny S3-anslutning",
            action: "app.new-s3",
        }),
    },
    VaultCategory {
        id: "known-hosts",
        label: "Kända värdar",
        add: None,
    },
];

/// Etiketterna i väljarens ordning.
pub fn labels() -> Vec<&'static str> {
    CATEGORIES.iter().map(|category| category.label).collect()
}

/// Kategorin på en viss plats i väljaren.
pub fn at(index: usize) -> Option<&'static VaultCategory> {
    CATEGORIES.get(index)
}

/// Platsen i väljaren för ett id — används när en åtgärd (`app.wireguard`,
/// `app.s3`) ska hoppa till sin kategori.
pub fn index_of(id: &str) -> Option<usize> {
    CATEGORIES.iter().position(|category| category.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_labels_are_unique() {
        for (index, category) in CATEGORIES.iter().enumerate() {
            for other in &CATEGORIES[index + 1..] {
                assert_ne!(category.id, other.id, "två kategorier delar id");
                assert_ne!(
                    category.label, other.label,
                    "två kategorier delar etikett — väljaren blir omöjlig att förstå"
                );
            }
        }
    }

    #[test]
    fn hosts_is_first_because_it_is_the_default() {
        assert_eq!(at(0).map(|c| c.id), Some("hosts"));
        assert_eq!(index_of("hosts"), Some(0));
        assert_eq!(index_of("finns-inte"), None);
        assert_eq!(labels().len(), CATEGORIES.len());
    }

    /// Det verkliga felet den här listan riskerar: ett id som inte
    /// motsvarar någon sida i stacken. `gtk::Stack::set_visible_child_name`
    /// på ett okänt namn gör INGENTING och säger ingenting — väljaren
    /// skulle bara sluta svara på just den raden. Samma teknik som
    /// `palette_actions`-testet: läs `main.rs` och kräv att sidan finns.
    #[test]
    fn every_category_has_a_stack_page_in_main() {
        let source = include_str!("main.rs");
        for category in CATEGORIES {
            let needle = format!("Some(\"{}\")", category.id);
            assert!(
                source.contains(&needle),
                "kategorin \"{}\" har ingen stack-sida i main.rs (letade efter {needle})",
                category.id
            );
        }
    }

    /// Samma fälla som stack-namnen, en våning upp: ett `+`-åtgärdsnamn
    /// som inte finns registrerat gör knappen tyst overksam (GTK gråar
    /// ut den utan att säga varför).
    #[test]
    fn every_add_button_action_is_registered_in_main() {
        let source = include_str!("main.rs");
        for category in CATEGORIES {
            let Some(add) = &category.add else { continue };
            let name = add
                .action
                .strip_prefix("app.")
                .expect("åtgärdsnamnet ska ha app.-prefix");
            let needle = format!("SimpleAction::new(\"{name}\"");
            assert!(
                source.contains(&needle),
                "kategorin \"{}\" pekar på åtgärden {} som inte finns i main.rs",
                category.id,
                add.action
            );
            assert!(
                !add.tooltip.is_empty(),
                "kategorin \"{}\" har en +-knapp utan verktygstips",
                category.id
            );
        }
    }
}
