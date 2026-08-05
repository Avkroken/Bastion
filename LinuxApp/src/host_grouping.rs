//! Ren (GTK-fri) grupperings-/filtreringslogik för värdlistan — utbruten ur
//! `main.rs` just för att kunna testas riktigt, till skillnad från resten
//! av filens GTK-limkod (som bara verifieras via en lyckad `cargo build`).
//! Port av `HostListModel.groups` + `HostListView.filteredGroups` i
//! `App/HostListView.swift`.

use crate::host::Host;

/// Grupperar värdar EXAKT som `HostListModel.groups`: favoriter (oavsett
/// tagg) i en egen sektion FÖRST ("★ Favoriter", bara om icke-tom), resten
/// grupperat per tagg (alfabetiskt, skiftlägesokänsligt) — en värd UTAN
/// taggar hamnar i en "Övriga"-sektion, en värd med FLERA taggar
/// förekommer i VARJE sin taggs sektion (matchar Swift-sidan rakt av, inte
/// en bugg). Varje sektions värdar sorteras på alias, skiftlägesokänsligt.
pub fn grouped_hosts(hosts: &[Host]) -> Vec<(String, Vec<Host>)> {
    let mut by_tag: std::collections::HashMap<String, Vec<Host>> = std::collections::HashMap::new();
    for h in hosts.iter().filter(|h| !h.is_favorite) {
        let tags: Vec<String> = if h.tags.is_empty() { vec!["Övriga".to_string()] } else { h.tags.clone() };
        for t in tags {
            by_tag.entry(t).or_default().push(h.clone());
        }
    }
    let mut tag_names: Vec<String> = by_tag.keys().cloned().collect();
    tag_names.sort_by_key(|t| t.to_lowercase());
    let mut groups: Vec<(String, Vec<Host>)> = tag_names
        .into_iter()
        .map(|t| {
            let mut hosts = by_tag.remove(&t).unwrap_or_default();
            hosts.sort_by_key(|h| h.alias.to_lowercase());
            (t, hosts)
        })
        .collect();
    let mut favorites: Vec<Host> = hosts.iter().filter(|h| h.is_favorite).cloned().collect();
    favorites.sort_by_key(|h| h.alias.to_lowercase());
    if !favorites.is_empty() {
        groups.insert(0, ("★ Favoriter".to_string(), favorites));
    }
    groups
}

/// Filtrerar grupperna på söktext — alias/värdnamn/användare/taggar,
/// skiftlägesokänsligt, matchar Swift-sidans `filteredGroups`. Tomma
/// sektioner (ingen träff i gruppen) faller bort helt, inte bara döljs.
pub fn filter_groups(groups: Vec<(String, Vec<Host>)>, query: &str) -> Vec<(String, Vec<Host>)> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return groups;
    }
    groups
        .into_iter()
        .filter_map(|(tag, hosts)| {
            let matched: Vec<Host> = hosts
                .into_iter()
                .filter(|h| {
                    h.alias.to_lowercase().contains(&needle)
                        || h.host_name.to_lowercase().contains(&needle)
                        || h.user.to_lowercase().contains(&needle)
                        || h.tags.iter().any(|t| t.to_lowercase().contains(&needle))
                })
                .collect();
            if matched.is_empty() { None } else { Some((tag, matched)) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(alias: &str, tags: &[&str], is_favorite: bool) -> Host {
        let mut h = Host::new(alias.to_string(), format!("{alias}.example.invalid"), "user".to_string());
        h.tags = tags.iter().map(|s| s.to_string()).collect();
        h.is_favorite = is_favorite;
        h
    }

    #[test]
    fn favorites_get_their_own_section_first_regardless_of_tags() {
        let hosts = vec![
            host("b-server", &["prod"], false),
            host("a-favorite", &["prod"], true),
            host("z-favorite", &[], true),
        ];
        let groups = grouped_hosts(&hosts);
        assert_eq!(groups[0].0, "★ Favoriter");
        // Sorterade på alias inom sektionen.
        assert_eq!(groups[0].1.iter().map(|h| h.alias.as_str()).collect::<Vec<_>>(), vec!["a-favorite", "z-favorite"]);
        // Favoriterna finns INTE dubblerat i "prod"-sektionen.
        let prod = groups.iter().find(|(tag, _)| tag == "prod").unwrap();
        assert_eq!(prod.1.iter().map(|h| h.alias.as_str()).collect::<Vec<_>>(), vec!["b-server"]);
    }

    #[test]
    fn no_favorites_section_when_none_are_favorited() {
        let hosts = vec![host("a", &["x"], false)];
        let groups = grouped_hosts(&hosts);
        assert!(groups.iter().all(|(tag, _)| tag != "★ Favoriter"));
    }

    #[test]
    fn untagged_hosts_land_in_ovriga() {
        let hosts = vec![host("no-tags", &[], false)];
        let groups = grouped_hosts(&hosts);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "Övriga");
    }

    #[test]
    fn a_host_with_multiple_tags_appears_in_every_one_of_its_tags_sections() {
        let hosts = vec![host("multi", &["work", "prod"], false)];
        let groups = grouped_hosts(&hosts);
        let tag_names: Vec<&str> = groups.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(tag_names, vec!["prod", "work"]); // alfabetisk ordning
        assert!(groups.iter().all(|(_, hs)| hs.iter().any(|h| h.alias == "multi")));
    }

    #[test]
    fn tag_sections_are_sorted_case_insensitively() {
        let hosts = vec![host("a", &["Zulu"], false), host("b", &["alpha"], false)];
        let groups = grouped_hosts(&hosts);
        let tag_names: Vec<&str> = groups.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(tag_names, vec!["alpha", "Zulu"]);
    }

    #[test]
    fn filter_matches_alias_hostname_user_or_tags_case_insensitively() {
        let hosts = vec![host("web-1", &["prod"], false)];
        let groups = grouped_hosts(&hosts);
        assert_eq!(filter_groups(groups.clone(), "WEB").len(), 1);
        assert_eq!(filter_groups(groups.clone(), "EXAMPLE.INVALID").len(), 1);
        assert_eq!(filter_groups(groups.clone(), "USER").len(), 1);
        assert_eq!(filter_groups(groups.clone(), "PROD").len(), 1);
        assert!(filter_groups(groups, "no-such-match").is_empty());
    }

    #[test]
    fn filter_drops_empty_sections_entirely_not_just_hides_them() {
        let hosts = vec![host("alpha", &["a"], false), host("beta", &["b"], false)];
        let groups = grouped_hosts(&hosts);
        let filtered = filter_groups(groups, "alpha");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "a");
    }

    #[test]
    fn empty_query_returns_every_group_unfiltered() {
        let hosts = vec![host("a", &["x"], false), host("b", &["y"], true)];
        let groups = grouped_hosts(&hosts);
        let filtered = filter_groups(groups.clone(), "   "); // bara blanktecken
        assert_eq!(filtered.len(), groups.len());
    }
}
