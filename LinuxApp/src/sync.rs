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

/// Det en post måste kunna svara på för att kunna slås ihop.
///
/// Utbruten när snippets tillkom i synken. Att kopiera hopslagningen per
/// typ vore inte bara upprepning — reglerna nedan (kommutativitet,
/// gravstenar, tidsstämpel-krockar) är subtila nog att en kopia förr
/// eller senare hade fått en av dem fel, och just den sortens fel
/// yttrar sig som tappad användardata utan felmeddelande.
pub trait Mergeable: Clone {
    fn id(&self) -> Uuid;
    fn modified_at(&self) -> f64;
    /// Stabil, ordningsoberoende nyckel för två poster med EXAKT samma
    /// `modified_at` — jämförelsen bryr sig bara om att den ger SAMMA svar
    /// för samma par oavsett i vilken ordning paret undersöks, inte om
    /// vilken av dem som "objektivt" är bäst (det finns inget sådant på en
    /// äkta tidsstämpel-krock).
    fn tiebreak_key(&self) -> String;
}

impl Mergeable for Host {
    fn id(&self) -> Uuid {
        self.id
    }
    fn modified_at(&self) -> f64 {
        self.modified_at.0
    }
    fn tiebreak_key(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

impl Mergeable for crate::snippet::Snippet {
    fn id(&self) -> Uuid {
        self.id
    }
    fn modified_at(&self) -> f64 {
        self.modified_at.0
    }
    fn tiebreak_key(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Behåller den nyaste kopian av varje id ur två listor.
fn newest_by_id<T: Mergeable>(a: Vec<T>, b: Vec<T>) -> HashMap<Uuid, T> {
    let mut newest: HashMap<Uuid, T> = HashMap::new();
    for item in a.into_iter().chain(b) {
        match newest.get(&item.id()) {
            Some(existing) => {
                // `>=` (eller `<` som villkor för att INTE ersätta) gjorde
                // detta ORDNINGSBEROENDE på en EXAKT tidsstämpel-krock: sist
                // sedd i kedjan (alltså `b`s kopia i `merge(a, b)`, men `a`s
                // i `merge(b, a)`) vann alltid — `merge(a, b) != merge(b, a)`
                // för just det fallet, ett brott mot kommutativitetslöftet
                // (CodeRabbit-fynd, samma bugg som WindowsApp/Bastion.Core/
                // SyncEngine.cs hade). Vid en RIKTIG krock avgörs det nu
                // istället av en stabil, ordningsoberoende jämförelse av
                // VÄRDET självt.
                let replace = match item.modified_at().partial_cmp(&existing.modified_at()) {
                    Some(std::cmp::Ordering::Greater) => true,
                    Some(std::cmp::Ordering::Equal) => {
                        item.tiebreak_key() > existing.tiebreak_key()
                    }
                    _ => false,
                };
                if replace {
                    newest.insert(item.id(), item);
                }
            }
            None => {
                newest.insert(item.id(), item);
            }
        }
    }
    newest
}

/// Posterna som överlever gravstenarna. En gravsten vinner om den är minst
/// lika ny som postens ändring; en NYARE ändring återupplivar posten.
fn survivors<T: Mergeable>(newest: &HashMap<Uuid, T>, tomb: &HashMap<Uuid, f64>) -> Vec<T> {
    newest
        .values()
        .filter(|item| match tomb.get(&item.id()) {
            Some(&deleted_at) => deleted_at < item.modified_at(),
            None => true,
        })
        .cloned()
        .collect()
}

/// Slår ihop två tillstånd deterministiskt utan server. Regler (identiska
/// med `SyncEngine.merge` i Swift):
/// - Samma värd på båda sidor: nyaste `modifiedAt` vinner (last-write-wins).
/// - Radering (gravsten) vinner om den är minst lika ny som värdens ändring;
///   annars "återupplivas" värden (en nyare redigering slår en äldre radering).
/// - Resultatet är kommutativt och idempotent — säkert att köra upprepat och
///   i valfri ordning mellan enheter.
pub fn merge(a: SyncState, b: SyncState) -> SyncState {
    let newest_hosts = newest_by_id(a.hosts, b.hosts);
    let newest_snippets = newest_by_id(a.snippets, b.snippets);

    let mut tomb: HashMap<Uuid, f64> = HashMap::new();
    for (id, t) in a.tombstones.into_iter().chain(b.tombstones) {
        let entry = tomb.entry(id).or_insert(f64::NEG_INFINITY);
        if t.0 > *entry {
            *entry = t.0;
        }
    }

    let mut live_hosts = survivors(&newest_hosts, &tomb);
    let mut live_snippets = survivors(&newest_snippets, &tomb);

    // En gravsten faller bara om en NYARE post med samma id lever — och den
    // posten kan vara av vilken typ som helst, eftersom gravstenarna delar
    // en karta. Att i stället behålla bara de gravstenar som saknar levande
    // värd (som tidigare) skulle tyst kasta varje gravsten som hörde till en
    // snippet, och då återuppstår raderade snippets vid nästa synk.
    let final_tombstones: HashMap<Uuid, crate::host::ReferenceDate> = tomb
        .into_iter()
        .filter(|(id, deleted_at)| {
            let revived_host = newest_hosts
                .get(id)
                .is_some_and(|h| h.modified_at() > *deleted_at);
            let revived_snippet = newest_snippets
                .get(id)
                .is_some_and(|s| s.modified_at() > *deleted_at);
            !revived_host && !revived_snippet
        })
        .map(|(id, t)| (id, crate::host::ReferenceDate(t)))
        .collect();

    live_hosts.sort_by_key(|h| h.alias.to_lowercase());
    live_snippets.sort_by_key(|s| s.name.to_lowercase());
    SyncState {
        hosts: live_hosts,
        snippets: live_snippets,
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
    /// Full URL till synkfilen på en WebDAV-server, t.ex. en Nextcloud
    /// eller `rclone serve webdav`. `None` betyder mapptransport.
    ///
    /// Alternativ till, inte utöver, mappen: en synk har en källa.
    #[serde(default)]
    pub webdav_url: Option<String>,
    /// Användarnamnet på WebDAV-servern. LÖSENORDET sparas aldrig här —
    /// samma regel som för lösenfrasen ovan, och av samma skäl: den här
    /// filen är oskyddad JSON i hemkatalogen.
    #[serde(default)]
    pub webdav_username: Option<String>,
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
            snippets: Vec::new(), tombstones: HashMap::new(),
        };
        let b = SyncState {
            hosts: vec![host_at("ny", id, 20.0)],
            snippets: Vec::new(), tombstones: HashMap::new(),
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
            snippets: Vec::new(), tombstones: HashMap::new(),
        };
        let mut tombstones = HashMap::new();
        tombstones.insert(id, ReferenceDate(20.0));
        let b = SyncState {
            hosts: vec![],
            snippets: Vec::new(),
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
            snippets: Vec::new(),
            tombstones,
        };
        let b = SyncState {
            hosts: vec![host_at("återupplivad", id, 20.0)],
            snippets: Vec::new(), tombstones: HashMap::new(),
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
            snippets: Vec::new(), tombstones: HashMap::new(),
        };
        let b = SyncState {
            hosts: vec![host_at("b", id, 20.0)],
            snippets: Vec::new(), tombstones: HashMap::new(),
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
        let a = SyncState { hosts: vec![host_at("alpha", id, 42.0)], snippets: Vec::new(), tombstones: HashMap::new() };
        let b = SyncState { hosts: vec![host_at("bravo", id, 42.0)], snippets: Vec::new(), tombstones: HashMap::new() };
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
        // Egna, tomma snippet-databaser: det här testet handlar om värdar,
        // och den enda synkvägen tar båda.
        let snip_dir = shared_path.parent().unwrap().to_path_buf();
        let mut snips_a =
            crate::snippet::SnippetStore::open(snip_dir.join("conv-a-snippets.json")).unwrap();
        let mut snips_b =
            crate::snippet::SnippetStore::open(snip_dir.join("conv-b-snippets.json")).unwrap();
        store_a.sync_with_snippets(&provider, &mut snips_a).unwrap();

        let host_from_b = Host::new("från-b".into(), "5.6.7.8".into(), "u".into());
        store_b.upsert(host_from_b.clone()).unwrap();
        store_b.sync_with_snippets(&provider, &mut snips_b).unwrap();

        // A synkar igen och ska nu se B:s värd också.
        store_a.sync_with_snippets(&provider, &mut snips_a).unwrap();

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
            webdav_url: None,
            webdav_username: None,
        };
        config.save(&path).unwrap();

        let reloaded = SyncConfig::load(&path);
        assert_eq!(
            reloaded.folder_path.as_deref(),
            Some("/mnt/syncthing/bastion")
        );
        std::fs::remove_dir_all(dir).ok();
    }

    /// En config skriven av en ÄLDRE version saknar webdav-fälten helt.
    /// `#[serde(default)]` ska göra det till None, inte till ett läsfel —
    /// annars tappar en uppgradering användarens synkmapp.
    #[test]
    fn a_config_written_before_webdav_existed_still_loads() {
        let dir = std::env::temp_dir().join(format!("bastion-syncconfig-old-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sync-config.json");
        std::fs::write(&path, r#"{"folder_path":"/mnt/gammal","encrypted":true}"#).unwrap();

        let loaded = SyncConfig::load(&path);
        assert_eq!(loaded.folder_path.as_deref(), Some("/mnt/gammal"));
        assert!(loaded.encrypted);
        assert_eq!(loaded.webdav_url, None);
        assert_eq!(loaded.webdav_username, None);
        std::fs::remove_dir_all(dir).ok();
    }

    /// WebDAV-uppgifterna ska överleva en tur till disk — men lösenordet
    /// finns inte ens som fält, så det KAN inte råka sparas.
    #[test]
    fn webdav_settings_round_trip_but_there_is_no_password_field_at_all() {
        let dir = std::env::temp_dir().join(format!("bastion-syncconfig-dav-{}", Uuid::new_v4()));
        let path = dir.join("sync-config.json");
        let config = SyncConfig {
            folder_path: None,
            encrypted: false,
            webdav_url: Some("https://moln.example/dav/bastion.json".into()),
            webdav_username: Some("anders".into()),
        };
        config.save(&path).unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("password"), "lösenordet får aldrig nå disk: {raw}");
        assert!(!raw.contains("hemligt"));

        let reloaded = SyncConfig::load(&path);
        assert_eq!(
            reloaded.webdav_url.as_deref(),
            Some("https://moln.example/dav/bastion.json")
        );
        assert_eq!(reloaded.webdav_username.as_deref(), Some("anders"));
        std::fs::remove_dir_all(dir).ok();
    }

    // MARK: Snippets

    fn snippet_at(name: &str, id: Uuid, modified_at: f64) -> crate::snippet::Snippet {
        let mut s = crate::snippet::Snippet::new(name.to_string(), format!("echo {name}"));
        s.id = id;
        s.modified_at = ReferenceDate(modified_at);
        s
    }

    /// Snippets följer EXAKT samma regler som värdar — det är hela poängen
    /// med att hopslagningen är utbruten i stället för kopierad. Faller något
    /// av de här fallen har de två typerna glidit isär.
    #[test]
    fn snippets_follow_the_same_last_write_wins_rule_as_hosts() {
        let id = Uuid::new_v4();
        let a = SyncState {
            hosts: vec![],
            snippets: vec![snippet_at("gammal", id, 10.0)],
            tombstones: HashMap::new(),
        };
        let b = SyncState {
            hosts: vec![],
            snippets: vec![snippet_at("ny", id, 20.0)],
            tombstones: HashMap::new(),
        };
        assert_eq!(merge(a, b).snippets[0].name, "ny");
    }

    /// En raderad snippet får INTE återuppstå. Gravstenarna delar karta med
    /// värdarna, och den första versionen av hopslagningen behöll bara de
    /// gravstenar som saknade levande VÄRD — vilket tyst hade kastat varje
    /// gravsten som hörde till en snippet.
    #[test]
    fn a_deleted_snippet_stays_deleted_through_a_merge() {
        let id = Uuid::new_v4();
        let mut tombstones = HashMap::new();
        tombstones.insert(id, ReferenceDate(30.0));
        let a = SyncState {
            hosts: vec![],
            snippets: vec![snippet_at("raderad", id, 10.0)],
            tombstones: HashMap::new(),
        };
        let b = SyncState { hosts: vec![], snippets: vec![], tombstones };

        let merged = merge(a, b);
        assert!(merged.snippets.is_empty(), "gravstenen ska vinna över den äldre versionen");
        assert!(
            merged.tombstones.contains_key(&id),
            "gravstenen måste ÖVERLEVA hopslagningen, annars återuppstår snippeten nästa varv"
        );

        // Och den ska hålla i sig varv efter varv.
        let again = merge(merged.clone(), merged);
        assert!(again.snippets.is_empty());
        assert!(again.tombstones.contains_key(&id));
    }

    /// En nyare redigering återupplivar en snippet, precis som för värdar.
    #[test]
    fn a_newer_snippet_edit_revives_it_over_an_older_tombstone() {
        let id = Uuid::new_v4();
        let mut tombstones = HashMap::new();
        tombstones.insert(id, ReferenceDate(10.0));
        let a = SyncState { hosts: vec![], snippets: vec![], tombstones };
        let b = SyncState {
            hosts: vec![],
            snippets: vec![snippet_at("aterupplivad", id, 20.0)],
            tombstones: HashMap::new(),
        };

        let merged = merge(a, b);
        assert_eq!(merged.snippets.len(), 1);
        assert!(!merged.tombstones.contains_key(&id), "gravstenen ska falla för den nyare ändringen");
    }

    /// Kommutativitet gäller hela tillståndet, inte bara värdarna.
    #[test]
    fn merge_is_commutative_with_snippets_and_hosts_mixed() {
        let host_id = Uuid::new_v4();
        let snip_id = Uuid::new_v4();
        let dead_id = Uuid::new_v4();
        let mut tombstones = HashMap::new();
        tombstones.insert(dead_id, ReferenceDate(50.0));

        let a = SyncState {
            hosts: vec![host_at("alpha", host_id, 10.0)],
            snippets: vec![snippet_at("ett", snip_id, 30.0)],
            tombstones: tombstones.clone(),
        };
        let b = SyncState {
            hosts: vec![host_at("alpha-nyare", host_id, 40.0)],
            snippets: vec![snippet_at("tva", snip_id, 20.0)],
            tombstones: HashMap::new(),
        };

        let ab = merge(a.clone(), b.clone());
        let ba = merge(b, a);
        assert_eq!(ab.hosts.len(), ba.hosts.len());
        assert_eq!(ab.hosts[0].alias, ba.hosts[0].alias);
        assert_eq!(ab.snippets[0].name, ba.snippets[0].name);
        assert_eq!(ab.snippets[0].name, "ett", "nyaste snippeten vinner");
        assert_eq!(ab.tombstones.len(), ba.tombstones.len());
    }

    /// Ett tomt snippet-fält får inte kasta bort motpartens snippets.
    #[test]
    fn merging_against_a_peer_without_snippets_keeps_ours() {
        let a = SyncState {
            hosts: vec![],
            snippets: vec![snippet_at("min", Uuid::new_v4(), 10.0)],
            tombstones: HashMap::new(),
        };
        let b = SyncState::default();
        assert_eq!(merge(a.clone(), b.clone()).snippets.len(), 1);
        assert_eq!(merge(b, a).snippets.len(), 1, "och åt andra hållet");
    }

    /// Slutbeviset: två helt oberoende par av databaser konvergerar på BÅDA
    /// posttyperna genom en delad mapp, inklusive en radering som måste
    /// hålla i sig. Motsvarar det befintliga värdtestet ovan.
    ///
    /// En radering är det som avslöjar en trasig synk: utan en gravsten som
    /// överlever hopslagningen kommer motparten glatt tillbaka med sin kopia,
    /// och snippeten återuppstår varje gång användaren raderar den.
    #[test]
    fn two_independent_stores_converge_on_snippets_too() {
        let dir = std::env::temp_dir().join(format!("bastion-snippet-sync-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // FolderSyncProvider pekar på en FIL i den delade mappen, inte på
        // mappen självt.
        let shared = dir.join("delad-sync.json");

        let mut hosts_a = crate::host::HostStore::open(dir.join("a-hosts.json")).unwrap();
        let mut snips_a = crate::snippet::SnippetStore::open(dir.join("a-snippets.json")).unwrap();
        let mut hosts_b = crate::host::HostStore::open(dir.join("b-hosts.json")).unwrap();
        let mut snips_b = crate::snippet::SnippetStore::open(dir.join("b-snippets.json")).unwrap();

        let provider = crate::sync::FolderSyncProvider::new(shared.clone());

        // A skapar två snippets, B en tredje.
        let doomed = crate::snippet::Snippet::new("doomed".into(), "echo doomed".into());
        let doomed_id = doomed.id;
        snips_a.upsert(doomed).unwrap();
        snips_a
            .upsert(crate::snippet::Snippet::new("kvar".into(), "echo kvar".into()))
            .unwrap();
        snips_b
            .upsert(crate::snippet::Snippet::new("bs-egna".into(), "echo b".into()))
            .unwrap();

        hosts_a.sync_with_snippets(&provider, &mut snips_a).unwrap();
        hosts_b.sync_with_snippets(&provider, &mut snips_b).unwrap();
        hosts_a.sync_with_snippets(&provider, &mut snips_a).unwrap();

        let names_a: Vec<String> = snips_a.all().iter().map(|s| s.name.clone()).collect();
        let names_b: Vec<String> = snips_b.all().iter().map(|s| s.name.clone()).collect();
        assert_eq!(names_a, names_b, "båda sidor ska se samma snippets");
        assert_eq!(names_a, vec!["bs-egna", "doomed", "kvar"]);

        // A raderar en. Raderingen måste överleva att B pushar tillbaka sin
        // kopia av samma snippet.
        snips_a.delete_synced(doomed_id, &mut hosts_a).unwrap();
        hosts_a.sync_with_snippets(&provider, &mut snips_a).unwrap();
        hosts_b.sync_with_snippets(&provider, &mut snips_b).unwrap();
        hosts_a.sync_with_snippets(&provider, &mut snips_a).unwrap();

        let names_a: Vec<String> = snips_a.all().iter().map(|s| s.name.clone()).collect();
        let names_b: Vec<String> = snips_b.all().iter().map(|s| s.name.clone()).collect();
        assert_eq!(names_a, vec!["bs-egna", "kvar"], "den raderade får inte återuppstå");
        assert_eq!(names_b, names_a, "och B ska se samma sak");

        std::fs::remove_dir_all(dir).ok();
    }

    /// `hosts.json` ska inte börja bära snippets bara för att synken
    /// hanterar dem — de bor i `snippets.json`. Två sanningskällor för
    /// samma data är hur de hinner glida isär.
    #[test]
    fn snippets_are_not_written_into_the_host_database_file() {
        let dir = std::env::temp_dir().join(format!("bastion-snippet-file-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let shared = dir.join("delad-sync.json");

        let hosts_path = dir.join("hosts.json");
        let mut hosts = crate::host::HostStore::open(hosts_path.clone()).unwrap();
        let mut snips = crate::snippet::SnippetStore::open(dir.join("snippets.json")).unwrap();
        snips
            .upsert(crate::snippet::Snippet::new("bara-har".into(), "echo x".into()))
            .unwrap();

        hosts
            .sync_with_snippets(&crate::sync::FolderSyncProvider::new(shared), &mut snips)
            .unwrap();

        let on_disk = std::fs::read_to_string(&hosts_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&on_disk).unwrap();
        assert_eq!(
            parsed["snippets"].as_array().map(|a| a.len()),
            Some(0),
            "hosts.json ska inte bära snippets, de bor i snippets.json"
        );
        assert_eq!(snips.all().len(), 1, "och snippeten ska fortfarande finnas kvar där");

        std::fs::remove_dir_all(dir).ok();
    }
}
