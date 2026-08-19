//! Port av Sources/SSHCore/Snippet.swift + SnippetStore.swift. Ett sparat
//! kommando med `{{variabel}}`-mall — inte bara text, kan fyllas i per
//! körning.

use crate::host::ReferenceDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: Uuid,
    pub name: String,
    pub template: String,
    pub modified_at: ReferenceDate,
}

impl Snippet {
    pub fn new(name: String, template: String) -> Self {
        Snippet {
            id: Uuid::new_v4(),
            name,
            template,
            modified_at: ReferenceDate::now(),
        }
    }

    /// Variabelnamnen i mallen (`{{namn}}`, mellanslag runt namnet trimmas),
    /// i den ordning de först förekommer, utan dubbletter.
    pub fn variable_names(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        occurrences(&self.template)
            .into_iter()
            .filter_map(|(_, name)| seen.insert(name.clone()).then_some(name))
            .collect()
    }

    /// Ersätter varje `{{namn}}`-förekomst med `values[namn]`. Saknade värden
    /// ersätts med tom sträng — en halvifylld snippet är fortfarande ett
    /// giltigt, om än ofullständigt, kommando att granska innan det skickas.
    pub fn rendered(&self, values: &std::collections::HashMap<String, String>) -> String {
        let mut result = String::new();
        let mut last_end = 0;
        for (range, name) in occurrences(&self.template) {
            result.push_str(&self.template[last_end..range.0]);
            result.push_str(values.get(&name).map(String::as_str).unwrap_or(""));
            last_end = range.1;
        }
        result.push_str(&self.template[last_end..]);
        result
    }
}

/// Hittar varje `{{ namn }}`-förekomst i mallen (byte-offsets), i den
/// ordning de står.
fn occurrences(template: &str) -> Vec<((usize, usize), String)> {
    let mut result = Vec::new();
    let mut search_start = 0;
    while let Some(open_rel) = template[search_start..].find("{{") {
        let open = search_start + open_rel;
        let Some(close_rel) = template[open + 2..].find("}}") else {
            break;
        };
        let close = open + 2 + close_rel;
        let name = template[open + 2..close].trim().to_string();
        if !name.is_empty() {
            result.push(((open, close + 2), name));
        }
        search_start = close + 2;
    }
    result
}

/// Persistent snippet-databas, `~/.bastion/snippets.json` — samma mönster
/// som `HostStore`, men en ren array.
///
/// Gravstenarna ligger MEDVETET inte här utan i sync-tillståndet
/// (`hosts.json`, som i sin helhet ÄR en serialiserad `SyncState`). Skälet
/// är att gravstenar bara betyder något för synken, och en delad karta för
/// båda posttyperna slipper både ett andra fält på tråden och en
/// formatändring av `snippets.json` som varje plattform hade behövt följa.
/// Radering av en snippet går därför via [`SnippetStore::delete_synced`],
/// som får sync-tillståndet med sig — `delete` utan gravsten finns kvar för
/// den som inte synkar, men en snippet raderad så ÅTERUPPSTÅR vid nästa
/// synk mot en enhet som fortfarande har den.
pub struct SnippetStore {
    path: std::path::PathBuf,
    snippets: Vec<Snippet>,
}

impl SnippetStore {
    pub fn default_path() -> std::path::PathBuf {
        dirs::home_dir()
            .expect("kunde inte hitta hemkatalogen")
            .join(".bastion/snippets.json")
    }

    pub fn open(path: std::path::PathBuf) -> std::io::Result<Self> {
        let snippets = match std::fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{}: {e}", path.display()),
                )
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(SnippetStore { path, snippets })
    }

    pub fn all(&self) -> Vec<&Snippet> {
        let mut s: Vec<&Snippet> = self.snippets.iter().collect();
        s.sort_by_key(|x| x.name.to_lowercase());
        s
    }

    pub fn upsert(&mut self, mut snippet: Snippet) -> std::io::Result<()> {
        snippet.modified_at = ReferenceDate::now();
        if let Some(existing) = self.snippets.iter_mut().find(|s| s.id == snippet.id) {
            *existing = snippet;
        } else {
            self.snippets.push(snippet);
        }
        self.persist()
    }

    pub fn delete(&mut self, id: Uuid) -> std::io::Result<()> {
        self.snippets.retain(|s| s.id != id);
        self.persist()
    }

    /// Raderar OCH skriver en gravsten i sync-tillståndet, så raderingen
    /// överlever en synk. Använd den här i allt som kan synkas; se
    /// typkommentaren för varför gravstenen bor i `HostStore`.
    pub fn delete_synced(
        &mut self,
        id: Uuid,
        state: &mut crate::host::HostStore,
    ) -> std::io::Result<()> {
        self.delete(id)?;
        state.record_tombstone(id)
    }

    /// Ersätter hela innehållet — används av synken när det sammanslagna
    /// resultatet ska skrivas tillbaka. Rör INTE `modified_at`, till skillnad
    /// från `upsert`: tidsstämplarna kommer från hopslagningen och är precis
    /// det som avgjorde vem som vann.
    pub fn replace_all(&mut self, snippets: Vec<Snippet>) -> std::io::Result<()> {
        self.snippets = snippets;
        self.persist()
    }

    fn persist(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let mut sorted = self.snippets.clone();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        crate::fsutil::atomic_write(
            &self.path,
            serde_json::to_string_pretty(&sorted)?.as_bytes(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn variable_names_are_in_first_occurrence_order_without_duplicates() {
        let s = Snippet::new(
            "t".into(),
            "docker compose restart {{service}} && journalctl -u {{service}} -n {{n}}".into(),
        );
        assert_eq!(
            s.variable_names(),
            vec!["service".to_string(), "n".to_string()]
        );
    }

    #[test]
    fn rendered_substitutes_values_and_trims_whitespace_in_names() {
        let s = Snippet::new("t".into(), "restart {{ service }}".into());
        let mut values = HashMap::new();
        values.insert("service".to_string(), "web".to_string());
        assert_eq!(s.rendered(&values), "restart web");
    }

    #[test]
    fn missing_values_render_as_empty_string_not_left_as_placeholder() {
        let s = Snippet::new("t".into(), "restart {{service}}".into());
        assert_eq!(s.rendered(&HashMap::new()), "restart ");
    }

    #[test]
    fn store_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("bastion-snippet-test-{}", Uuid::new_v4()));
        let path = dir.join("snippets.json");
        let mut store = SnippetStore::open(path.clone()).unwrap();
        store
            .upsert(Snippet::new(
                "Restart web".into(),
                "docker compose restart {{service}}".into(),
            ))
            .unwrap();

        let reopened = SnippetStore::open(path).unwrap();
        assert_eq!(reopened.all().len(), 1);
        assert_eq!(reopened.all()[0].name, "Restart web");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_corrupt_snippets_json_is_an_error_not_a_silent_empty_state() {
        let dir = std::env::temp_dir().join(format!("bastion-snippet-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("snippets.json");
        std::fs::write(&path, "{ inte giltig json").unwrap();

        let result = SnippetStore::open(path);
        assert!(
            result.is_err(),
            "en trunkerad/skadad fil ska propagera ett fel, inte tyst bli en tom lista"
        );
        std::fs::remove_dir_all(dir).ok();
    }
}
