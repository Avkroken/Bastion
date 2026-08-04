//! Klientoberoende synkprotokoll — port av `Sources/SSHCore/SyncEngine.swift`
//! + `SyncProvider.swift`. Se `SYNC_PROTOCOL.md` i repo-roten för den
//! formella specifikationen (wire-format, merge-semantik, transport-
//! abstraktionen) som ALLA klienter (App/, Android, LinuxApp, framtida
//! WindowsApp) förväntas implementera för att delta.
//!
//! Medvetet UTELÄMNAT här (finns i Swift-sidans molnleverantörer, se
//! `Sources/SSHCore/*SyncProvider.swift` + `SyncCrypto.swift`): kryptering
//! av `SyncState` innan uppladdning till en icke-betrodd molntransport
//! (Dropbox/Drive/OneDrive). `FolderSyncProvider` här är den enkla,
//! okrypterade varianten — rätt för en lokal mapp som redan synkas av
//! något annat (Syncthing, en klonad Git-mapp, en krypterad disk) men INTE
//! rätt för att skicka rakt upp till en molntjänst man inte litar på.
//! Kryptering är en framtida transport-specifik utbyggnad, inte en ändring
//! av kärnprotokollet.

use crate::host::{Host, SyncState};
use std::collections::HashMap;
use uuid::Uuid;

/// Stabil, ordningsoberoende tiebreak-nyckel för två `Host`-värden med
/// EXAKT samma `modified_at` — jämförelsen bryr sig bara om att den ger
/// SAMMA svar för samma par oavsett i vilken ordning paret undersöks, inte
/// om vilken av dem som "objektivt" är bäst (det finns inget sådant på en
/// äkta tidsstämpel-krock).
fn tiebreak_key(h: &Host) -> String {
    serde_json::to_string(h).unwrap_or_default()
}

/// Slår ihop två tillstånd deterministiskt utan server. Regler (identiska
/// med `SyncEngine.merge` i Swift):
/// - Samma värd på båda sidor: nyaste `modifiedAt` vinner (last-write-wins).
/// - Radering (gravsten) vinner om den är minst lika ny som värdens ändring;
///   annars "återupplivas" värden (en nyare redigering slår en äldre radering).
/// - Resultatet är kommutativt och idempotent — säkert att köra upprepat och
///   i valfri ordning mellan enheter.
pub fn merge(a: SyncState, b: SyncState) -> SyncState {
    let mut newest_host: HashMap<Uuid, Host> = HashMap::new();
    for h in a.hosts.into_iter().chain(b.hosts) {
        match newest_host.get(&h.id) {
            Some(existing) => {
                // `>=` (eller `<` som villkor för att INTE ersätta) gjorde
                // detta ORDNINGSBEROENDE på en EXAKT tidsstämpel-krock: sist
                // sedd i kedjan (alltså `b`s kopia i `merge(a, b)`, men `a`s
                // i `merge(b, a)`) vann alltid — `merge(a, b) != merge(b, a)`
                // för just det fallet, ett brott mot kommutativitetslöftet
                // ovan (CodeRabbit-fynd, samma bugg som WindowsApp/
                // Bastion.Core/SyncEngine.cs hade). Vid en RIKTIG krock
                // avgörs det nu istället av en stabil, ordningsoberoende
                // jämförelse av VÄRDET självt (JSON-serialiseringen) — samma
                // par (h, existing) ger samma vinnare oavsett i vilken
                // ordning de råkade besökas.
                let replace = match h.modified_at.0.partial_cmp(&existing.modified_at.0) {
                    Some(std::cmp::Ordering::Greater) => true,
                    Some(std::cmp::Ordering::Equal) => tiebreak_key(&h) > tiebreak_key(existing),
                    _ => false,
                };
                if replace {
                    newest_host.insert(h.id, h);
                }
            }
            None => {
                newest_host.insert(h.id, h);
            }
        }
    }

    let mut tomb: HashMap<Uuid, f64> = HashMap::new();
    for (id, t) in a.tombstones.into_iter().chain(b.tombstones) {
        let entry = tomb.entry(id).or_insert(f64::NEG_INFINITY);
        if t.0 > *entry {
            *entry = t.0;
        }
    }

    let mut live_hosts = Vec::new();
    let mut final_tombstones = HashMap::new();
    let all_ids: std::collections::HashSet<Uuid> = newest_host
        .keys()
        .copied()
        .chain(tomb.keys().copied())
        .collect();
    for id in all_ids {
        match (newest_host.get(&id), tomb.get(&id)) {
            (Some(host), Some(&deleted_at)) => {
                if deleted_at >= host.modified_at.0 {
                    final_tombstones.insert(id, crate::host::ReferenceDate(deleted_at));
                } else {
                    live_hosts.push(host.clone());
                }
            }
            (Some(host), None) => live_hosts.push(host.clone()),
            (None, Some(&deleted_at)) => {
                final_tombstones.insert(id, crate::host::ReferenceDate(deleted_at));
            }
            (None, None) => {}
        }
    }
    live_hosts.sort_by_key(|h| h.alias.to_lowercase());
    SyncState {
        hosts: live_hosts,
        tombstones: final_tombstones,
    }
}

/// En synktransport: hämta fjärrtillstånd och skriv tillbaka det
/// sammanslagna. Motsvarar `protocol SyncProvider` i Swift.
pub trait SyncProvider {
    fn pull(&self) -> std::io::Result<Option<SyncState>>;
    fn push(&self, state: &SyncState) -> std::io::Result<()>;
}

/// Enklaste transporten: en JSON-fil i en mapp som något annat synkar
/// mellan enheter (Syncthing, en klonad Git-mapp, en krypterad disk …).
/// Motsvarar `FolderSyncProvider` i Swift — samma wire-format, så en
/// `LinuxApp`- och en `App/`-instans kan synka via samma mapp/fil.
pub struct FolderSyncProvider {
    path: std::path::PathBuf,
}

impl FolderSyncProvider {
    pub fn new(path: std::path::PathBuf) -> Self {
        FolderSyncProvider { path }
    }
}

impl SyncProvider for FolderSyncProvider {
    fn pull(&self) -> std::io::Result<Option<SyncState>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&self.path)?;
        let state = serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Some(state))
    }

    fn push(&self, state: &SyncState) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let json = serde_json::to_string_pretty(state)?;
        crate::fsutil::atomic_write(&self.path, json.as_bytes())
    }
}

/// Klientlokal inställning — VILKEN mapp den här installationen synkar
/// mot. Medvetet INTE en del av det delade `SyncState`/protokollet: varje
/// klient/enhet kan (och bör kunna) peka mot en annan lokal
/// synk-mapp/monteringspunkt, det är inte data att slå ihop mellan enheter.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncConfig {
    pub folder_path: Option<String>,
    /// Om synkfilen ska krypteras (`sync_crypto::EncryptedFolderSyncProvider`)
    /// — rätt för en tredjeparts molnmapp (Dropbox/Drive/OneDrive) man inte
    /// litar på blint. Lösenfrasen SPARAS ALDRIG här — bara att kryptering
    /// önskas, frasen matas in på nytt varje "Synka nu".
    #[serde(default)]
    pub encrypted: bool,
}

impl SyncConfig {
    pub fn default_path() -> std::path::PathBuf {
        dirs::home_dir()
            .expect("kunde inte hitta hemkatalogen")
            .join(".bastion/sync-config.json")
    }

    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        crate::fsutil::atomic_write(path, serde_json::to_string_pretty(self)?.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{Host, HostAuth, ReferenceDate};

    fn host_at(alias: &str, id: Uuid, modified_at: f64) -> Host {
        let mut h = Host::new(alias.into(), "h".into(), "u".into());
        h.id = id;
        h.modified_at = ReferenceDate(modified_at);
        h
    }

    #[test]
    fn newer_edit_wins_over_older_edit() {
        let id = Uuid::new_v4();
        let a = SyncState {
            hosts: vec![host_at("gammal", id, 10.0)],
            tombstones: HashMap::new(),
        };
        let b = SyncState {
            hosts: vec![host_at("ny", id, 20.0)],
            tombstones: HashMap::new(),
        };
        let merged = merge(a, b);
        assert_eq!(merged.hosts.len(), 1);
        assert_eq!(merged.hosts[0].alias, "ny");
    }

    #[test]
    fn tombstone_wins_when_newer_than_edit() {
        let id = Uuid::new_v4();
        let a = SyncState {
            hosts: vec![host_at("host", id, 10.0)],
            tombstones: HashMap::new(),
        };
        let mut tombstones = HashMap::new();
        tombstones.insert(id, ReferenceDate(20.0));
        let b = SyncState {
            hosts: vec![],
            tombstones,
        };
        let merged = merge(a, b);
        assert!(merged.hosts.is_empty());
        assert_eq!(merged.tombstones.len(), 1);
    }

    #[test]
    fn newer_edit_revives_over_older_tombstone() {
        let id = Uuid::new_v4();
        let mut tombstones = HashMap::new();
        tombstones.insert(id, ReferenceDate(10.0));
        let a = SyncState {
            hosts: vec![],
            tombstones,
        };
        let b = SyncState {
            hosts: vec![host_at("återupplivad", id, 20.0)],
            tombstones: HashMap::new(),
        };
        let merged = merge(a, b);
        assert_eq!(merged.hosts.len(), 1);
        assert_eq!(merged.hosts[0].alias, "återupplivad");
        assert!(merged.tombstones.is_empty());
    }

    #[test]
    fn merge_is_commutative() {
        let id = Uuid::new_v4();
        let a = SyncState {
            hosts: vec![host_at("a", id, 10.0)],
            tombstones: HashMap::new(),
        };
        let b = SyncState {
            hosts: vec![host_at("b", id, 20.0)],
            tombstones: HashMap::new(),
        };
        let ab = merge(a.clone(), b.clone());
        let ba = merge(b, a);
        assert_eq!(ab.hosts.len(), ba.hosts.len());
        assert_eq!(ab.hosts[0].alias, ba.hosts[0].alias);
    }

    /// Regressionstest för en ORDNINGSBEROENDE tie-bugg (CodeRabbit-fynd):
    /// på en EXAKT tidsstämpel-krock (samma `modified_at`) vann tidigare
    /// alltid den sist besökta kopian i kedjan — `merge(a, b)` gav ett
    /// annat resultat än `merge(b, a)`, ett brott mot kommutativitetslöftet
    /// som `merge_is_commutative` ovan aldrig fångade (den använder olika
    /// tidsstämplar, ingen äkta krock).
    #[test]
    fn merge_is_commutative_even_on_an_exact_modified_at_tie() {
        let id = Uuid::new_v4();
        let a = SyncState { hosts: vec![host_at("alpha", id, 42.0)], tombstones: HashMap::new() };
        let b = SyncState { hosts: vec![host_at("bravo", id, 42.0)], tombstones: HashMap::new() };
        let ab = merge(a.clone(), b.clone());
        let ba = merge(b, a);
        assert_eq!(ab.hosts.len(), 1);
        assert_eq!(
            ab.hosts[0].alias, ba.hosts[0].alias,
            "merge(a, b) och merge(b, a) ska ge SAMMA vinnare på en exakt tidsstämpel-krock"
        );
    }

    /// Riktig cross-instans-verifiering: två oberoende HostStores, var sin
    /// egen fil, synkar via en gemensam FolderSyncProvider-fil och
    /// KONVERGERAR till samma tillstånd — inte bara `merge()` isolerat.
    #[test]
    fn two_independent_stores_converge_through_a_shared_folder_provider() {
        use crate::host::HostStore;

        let dir = std::env::temp_dir().join(format!("bastion-sync-test-{}", Uuid::new_v4()));
        let store_a_path = dir.join("a/hosts.json");
        let store_b_path = dir.join("b/hosts.json");
        let shared_path = dir.join("shared/hosts.json");

        let mut store_a = HostStore::open(store_a_path.clone()).unwrap();
        let mut store_b = HostStore::open(store_b_path.clone()).unwrap();

        let mut host_from_a = Host::new("från-a".into(), "1.2.3.4".into(), "u".into());
        host_from_a.auth = HostAuth::KeyFile("/x".into());
        store_a.upsert(host_from_a.clone()).unwrap();

        let provider = FolderSyncProvider::new(shared_path.clone());
        store_a.sync(&provider).unwrap();

        let host_from_b = Host::new("från-b".into(), "5.6.7.8".into(), "u".into());
        store_b.upsert(host_from_b.clone()).unwrap();
        store_b.sync(&provider).unwrap();

        // A synkar igen och ska nu se B:s värd också.
        store_a.sync(&provider).unwrap();

        let aliases_a: Vec<String> = store_a.all().iter().map(|h| h.alias.clone()).collect();
        assert!(aliases_a.contains(&"från-a".to_string()));
        assert!(
            aliases_a.contains(&"från-b".to_string()),
            "A såg aldrig B:s värd efter synk"
        );

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn sync_config_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("bastion-syncconfig-test-{}", Uuid::new_v4()));
        let path = dir.join("sync-config.json");
        let config = SyncConfig {
            folder_path: Some("/mnt/syncthing/bastion".into()),
            encrypted: false,
        };
        config.save(&path).unwrap();

        let reloaded = SyncConfig::load(&path);
        assert_eq!(
            reloaded.folder_path.as_deref(),
            Some("/mnt/syncthing/bastion")
        );
        std::fs::remove_dir_all(dir).ok();
    }
}
