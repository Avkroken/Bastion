//! Luddig matchning för kommandopaletten.
//!
//! Poängen med en palett är att `pw1` ska hitta `prod-web-1` utan att man
//! skriver hela namnet. Reglerna är avsiktligt få och förutsägbara — en
//! palett som rankar oförklarligt är värre än ingen palett alls:
//!
//! * Alla tecken i sökningen måste finnas i kandidaten, i ordning.
//! * Träffar som sitter ihop är bättre än utspridda.
//! * Träffar i början av ett ord är bättre än mitt inne i ett.
//!
//! Gemener/versaler ignoreras. Modulen är GTK-fri och därmed testbar —
//! `main.rs` har ingen `#[cfg(test)]`-täckning av hävd.

/// Tecken som räknas som ordgräns i ett värdnamn: `prod-web-1`,
/// `root@10.0.0.5`, `db_backup`, `web.example.com`.
fn is_boundary(c: char) -> bool {
    matches!(c, '-' | '_' | '.' | '@' | ':' | '/' | ' ')
}

/// Hur väl `query` matchar `candidate`. `None` betyder ingen träff alls.
/// Högre är bättre; värdena är bara meningsfulla i jämförelse med andra
/// kandidaters poäng för SAMMA sökning.
pub fn score(candidate: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let haystack: Vec<char> = candidate.to_lowercase().chars().collect();
    let needle: Vec<char> = query.to_lowercase().chars().collect();

    let mut total = 0;
    let mut position = 0usize;
    let mut previous_match: Option<usize> = None;

    for wanted in needle {
        let found = haystack[position..].iter().position(|c| *c == wanted)? + position;

        // Sitter träffen direkt efter föregående är det ett sammanhängande
        // fragment — det är den starkaste signalen om att användaren
        // skriver början på ett namn.
        if previous_match == Some(found.wrapping_sub(1)) {
            total += 12;
        }

        // Början av kandidaten, eller början av ett ord inuti den.
        if found == 0 || is_boundary(haystack[found - 1]) {
            total += 9;
        }

        // Ju längre fram träffen ligger, desto svagare — men aldrig så
        // mycket att en riktig träff hamnar under noll.
        total += (6 - found.min(6)) as i32;

        previous_match = Some(found);
        position = found + 1;
    }

    // Ett kort namn som matchar helt är oftast det man menade.
    if haystack.len() == needle_len(query) {
        total += 15;
    }

    Some(total)
}

fn needle_len(query: &str) -> usize {
    query.to_lowercase().chars().count()
}

/// Kandidaterna som matchar, bäst först. Lika poäng behåller inbördes
/// ordning (stabil sortering), så en lista som redan är vettigt ordnad
/// inte kastas om utan skäl.
pub fn rank<'a>(candidates: &[(&'a str, usize)], query: &str) -> Vec<(&'a str, usize)> {
    let mut scored: Vec<(i32, &'a str, usize)> = candidates
        .iter()
        .filter_map(|(text, id)| score(text, query).map(|s| (s, *text, *id)))
        .collect();
    scored.sort_by_key(|(points, _, _)| std::cmp::Reverse(*points));
    scored.into_iter().map(|(_, text, id)| (text, id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(score("prod-web-1", ""), Some(0));
    }

    #[test]
    fn missing_character_is_no_match() {
        assert_eq!(score("prod-web-1", "xyz"), None);
    }

    #[test]
    fn characters_must_appear_in_order() {
        assert!(score("prod-web", "bew").is_none());
        assert!(score("prod-web", "pweb").is_some());
    }

    #[test]
    fn initials_across_words_match() {
        // Själva poängen med en palett.
        assert!(score("prod-web-1", "pw1").is_some());
    }

    #[test]
    fn case_is_ignored() {
        assert!(score("PROD-Web", "prodweb").is_some());
    }

    #[test]
    fn contiguous_beats_scattered() {
        let together = score("webserver", "web").unwrap();
        let apart = score("wxexbserver", "web").unwrap();
        assert!(
            together > apart,
            "sammanhängande {together} borde slå utspridd {apart}"
        );
    }

    #[test]
    fn word_start_beats_mid_word() {
        let boundary = score("prod-web", "web").unwrap();
        let inside = score("prodxweb", "web").unwrap();
        assert!(
            boundary > inside,
            "ordbörjan {boundary} borde slå inuti ordet {inside}"
        );
    }

    #[test]
    fn exact_short_name_wins_over_long_one() {
        let exact = score("web", "web").unwrap();
        let longer = score("web-server-production", "web").unwrap();
        assert!(exact > longer, "exakt {exact} borde slå längre {longer}");
    }

    #[test]
    fn ranking_puts_the_obvious_choice_first() {
        let candidates = [
            ("db-backup", 0usize),
            ("prod-web-1", 1),
            ("staging-web", 2),
            ("web", 3),
        ];
        let ranked = rank(&candidates, "web");
        assert_eq!(ranked.first().map(|(t, _)| *t), Some("web"));
        // db-backup saknar w/e/b i ordning och ska filtreras bort helt.
        assert!(!ranked.iter().any(|(t, _)| *t == "db-backup"));
    }

    #[test]
    fn equal_scores_keep_their_original_order() {
        // Kommandopaletten förlitar sig på det här: öppna sessioner läggs
        // in före värdlistan och ska ligga kvar där när poängen är lika,
        // så att "byt till fliken du redan har" vinner över "öppna en till".
        let candidates = [("web", 0usize), ("web", 1), ("web", 2)];
        let ranked = rank(&candidates, "web");
        assert_eq!(ranked, vec![("web", 0), ("web", 1), ("web", 2)]);
    }

    #[test]
    fn ranking_keeps_identifiers_with_their_text() {
        let candidates = [("alpha", 7usize), ("beta", 9)];
        let ranked = rank(&candidates, "bet");
        assert_eq!(ranked, vec![("beta", 9)]);
    }
}
