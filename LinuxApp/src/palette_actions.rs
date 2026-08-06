//! Appens åtgärder som poster i kommandopaletten.
//!
//! Paletten kunde från början bara nå värdar och öppna sessioner. Allt
//! annat appen gör — importera en ssh-config, öppna S3-anslutningarna,
//! ändra inställningar — låg bakom primärmenyn och krävde musen. Det är
//! precis den halvan en palett finns till för.
//!
//! Åtgärderna beskrivs här som ren data av två skäl. Dels blir listan
//! möjlig att testa, vilket `main.rs` inte är (ingen `#[cfg(test)]`-
//! täckning av hävd). Dels blir det ett ställe att lägga SÖKORD på:
//! menyposten heter "Funktioner", men den som letar efter den skriver
//! rimligen "inställningar". Etiketten ska matcha menyn — sökorden får
//! täcka in vad man faktiskt skriver.

/// En åtgärd som går att aktivera från paletten.
pub struct CommandAction {
    /// Samma text som i menyn. Två namn på samma sak är värre än ett
    /// klumpigt namn.
    pub label: &'static str,
    /// Fullständigt åtgärdsnamn, `app.`-prefixet inkluderat — det är den
    /// formen `ActionGroup::activate_action` vill ha.
    pub action: &'static str,
    /// Extra ord som ska hitta åtgärden, utöver etiketten. Synonymer och
    /// engelska namn på det som har svensk etikett.
    pub keywords: &'static str,
    /// Åtgärder som inte betyder något utan en öppen session göms när
    /// inga flikar finns — en palettrad som garanterat inte gör något är
    /// bara brus.
    pub needs_session: bool,
}

/// Alla åtgärder, i den ordning de visas vid tom sökning: det man gör
/// ofta först. `palette` och `focus-search` är med avsikt INTE med —
/// paletten öppnar inte sig själv, och att söka sig fram till en annan
/// sökruta är en omväg.
const ACTIONS: &[CommandAction] = &[
    CommandAction {
        label: "Ny värd",
        action: "app.new-host",
        keywords: "lägg till host server skapa new",
        needs_session: false,
    },
    CommandAction {
        label: "Snabbanslut",
        action: "app.new-connection",
        keywords: "anslut ny anslutning quick connect ssh",
        needs_session: false,
    },
    CommandAction {
        label: "Telnet",
        action: "app.telnet",
        keywords: "anslut anslutning",
        needs_session: false,
    },
    CommandAction {
        label: "Seriell/USB",
        action: "app.serial",
        keywords: "serial konsol console tty port",
        needs_session: false,
    },
    CommandAction {
        label: "Tailscale",
        action: "app.tailscale",
        keywords: "nätverk vpn mesh enheter",
        needs_session: false,
    },
    CommandAction {
        label: "WireGuard-profiler",
        action: "app.wireguard",
        keywords: "nätverk vpn tunnel profil",
        needs_session: false,
    },
    CommandAction {
        label: "S3-anslutningar",
        action: "app.s3",
        keywords: "lagring bucket objekt minio storage",
        needs_session: false,
    },
    CommandAction {
        label: "Importera ssh-config",
        action: "app.import_ssh_config",
        keywords: "ssh_config import öppna fil",
        needs_session: false,
    },
    CommandAction {
        label: "Funktioner",
        action: "app.settings",
        keywords: "inställningar settings preferenser val",
        needs_session: false,
    },
    CommandAction {
        label: "Stäng fliken",
        action: "app.close-tab",
        keywords: "close tab avsluta session",
        needs_session: true,
    },
];

/// Åtgärderna som är meningsfulla just nu.
pub fn available(has_open_session: bool) -> impl Iterator<Item = &'static CommandAction> {
    ACTIONS
        .iter()
        .filter(move |action| has_open_session || !action.needs_session)
}

/// Det som söks i för en åtgärd: etiketten plus sökorden. Fuzzy-matchning
/// kräver att tecknen kommer i ordning, så orden får inte klumpas ihop —
/// mellanslag är ordgräns även för poängsättningen.
pub fn haystack(action: &CommandAction) -> String {
    format!("{} {}", action.label, action.keywords)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuzzy;

    /// Bäst matchande åtgärdens etikett för en sökning, som paletten
    /// själv skulle rangordna den.
    fn best_match(query: &str) -> Option<&'static str> {
        let haystacks: Vec<String> = available(true).map(haystack).collect();
        let candidates: Vec<(&str, usize)> = haystacks
            .iter()
            .enumerate()
            .map(|(index, text)| (text.as_str(), index))
            .collect();
        let ranked = fuzzy::rank(&candidates, query);
        ranked
            .first()
            .map(|(_, index)| available(true).nth(*index).unwrap().label)
    }

    /// Det verkliga felet den här listan riskerar: ett åtgärdsnamn som
    /// stavats fel eller som `main.rs` senare döper om. GTK ritar en
    /// sådan post utan att klaga — den gör bara ingenting när man klickar
    /// på den. Testet läser `main.rs` och kräver att varje namn finns
    /// registrerat där.
    #[test]
    fn every_action_is_registered_in_main() {
        let source = include_str!("main.rs");
        for action in ACTIONS {
            let bare = action
                .action
                .strip_prefix("app.")
                .unwrap_or_else(|| panic!("{} saknar app.-prefix", action.action));
            let registration = format!("SimpleAction::new(\"{bare}\"");
            assert!(
                source.contains(&registration),
                "åtgärden {} finns inte registrerad i main.rs ({registration})",
                action.action
            );
        }
    }

    #[test]
    fn action_names_are_unique() {
        let mut names: Vec<&str> = ACTIONS.iter().map(|a| a.action).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(count, names.len(), "samma åtgärd finns med två gånger");
    }

    #[test]
    fn the_palette_never_offers_to_open_itself() {
        // Annars går det att öppna paletten från paletten, i all
        // oändlighet.
        assert!(!ACTIONS.iter().any(|a| a.action == "app.palette"));
    }

    #[test]
    fn tab_actions_are_hidden_without_an_open_session() {
        let without: Vec<&str> = available(false).map(|a| a.label).collect();
        let with: Vec<&str> = available(true).map(|a| a.label).collect();
        assert!(!without.contains(&"Stäng fliken"));
        assert!(with.contains(&"Stäng fliken"));
    }

    #[test]
    fn synonyms_find_the_action_behind_its_menu_name() {
        // Hela skälet till att sökorden finns: menyn säger "Funktioner",
        // men det man skriver är "inställningar".
        assert_eq!(best_match("inställningar"), Some("Funktioner"));
    }

    #[test]
    fn english_names_find_the_swedish_label() {
        assert_eq!(best_match("settings"), Some("Funktioner"));
    }

    #[test]
    fn a_prefix_of_the_label_finds_the_action() {
        assert_eq!(best_match("wire"), Some("WireGuard-profiler"));
        assert_eq!(best_match("telnet"), Some("Telnet"));
    }

    #[test]
    fn ssh_config_import_is_findable_by_the_file_name() {
        assert_eq!(best_match("ssh_config"), Some("Importera ssh-config"));
    }

    #[test]
    fn every_action_has_a_label_and_a_haystack_that_contains_it() {
        for action in ACTIONS {
            assert!(!action.label.trim().is_empty());
            let text = haystack(action);
            assert!(
                text.starts_with(action.label),
                "sökunderlaget för {} borde börja med etiketten",
                action.label
            );
        }
    }
}
