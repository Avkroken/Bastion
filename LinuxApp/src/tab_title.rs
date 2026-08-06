//! Namnsättning av sessionsflikar.
//!
//! Logiken låg tidigare inbäddad i `main.rs` bland GTK-anropen och kunde
//! därför inte testas — `main.rs` har ingen `#[cfg(test)]`-täckning alls,
//! av gammal hävd. Här är den ren: in kommer strängar, ut kommer en
//! sträng, och båda felen som ledde hit har ett test som visar dem.

/// Fliknamnet en session ska utgå ifrån.
///
/// Snabbanslutningar sparas aldrig i värdlistan och får därför ett tomt
/// alias (`Host::new(String::new(), ...)`). Det gick rakt in i fliknamnet,
/// så fliken hette bokstavligen ingenting — eller " (2)" när det fanns
/// fler än en. Faller nu tillbaka på `användare@värd`, samma form som
/// `ssh` självt och som prompten på andra sidan visar.
pub fn base_title(alias: &str, user: &str, host_name: &str) -> String {
    if alias.trim().is_empty() {
        format!("{user}@{host_name}")
    } else {
        alias.to_string()
    }
}

/// Gör namnet unikt mot de flikar som redan är öppna: `web`, `web (2)`,
/// `web (3)`. Två flikar mot samma värd ska gå att skilja åt.
pub fn unique_title(base: &str, existing: &[String]) -> String {
    let prefix = format!("{base} (");
    let taken = existing
        .iter()
        .filter(|title| title.as_str() == base || title.starts_with(&prefix))
        .count();

    if taken == 0 {
        base.to_string()
    } else {
        format!("{base} ({})", taken + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_used_when_present() {
        assert_eq!(base_title("web", "root", "10.0.0.5"), "web");
    }

    #[test]
    fn empty_alias_falls_back_to_user_at_host() {
        // Regressionen: snabbanslutningens flik hette tidigare "".
        assert_eq!(base_title("", "root", "10.0.0.5"), "root@10.0.0.5");
    }

    #[test]
    fn blank_alias_counts_as_empty() {
        assert_eq!(base_title("   ", "root", "10.0.0.5"), "root@10.0.0.5");
    }

    #[test]
    fn first_tab_keeps_the_plain_name() {
        assert_eq!(unique_title("web", &[]), "web");
    }

    #[test]
    fn second_and_third_tab_are_numbered() {
        let one = vec!["web".to_string()];
        assert_eq!(unique_title("web", &one), "web (2)");

        let two = vec!["web".to_string(), "web (2)".to_string()];
        assert_eq!(unique_title("web", &two), "web (3)");
    }

    #[test]
    fn other_hosts_do_not_count() {
        let existing = vec!["db".to_string(), "db (2)".to_string()];
        assert_eq!(unique_title("web", &existing), "web");
    }

    #[test]
    fn quick_connect_tabs_to_the_same_host_are_distinguishable() {
        // Hela kedjan: två snabbanslutningar mot samma värd gav förut
        // "" och " (2)". Nu blir de läsbara och åtskilda.
        let first = unique_title(&base_title("", "root", "10.0.0.5"), &[]);
        assert_eq!(first, "root@10.0.0.5");

        let second = unique_title(&base_title("", "root", "10.0.0.5"), &[first.clone()]);
        assert_eq!(second, "root@10.0.0.5 (2)");
    }
}
