//! Wire-kompatibel Rust-motsvarighet till Sources/SSHCore/Host.swift +
//! HostStore.swift + SyncEngine.swift (SyncState). Format verifierat empiriskt
//! mot Swifts JSONEncoder (inte gissat):
//! - `Date` kodas som f64-sekunder sedan referensdatumet 2001-01-01T00:00:00Z,
//!   INTE Unix-epok.
//! - Enum med associerat värde kodas som `{"case": {"_0": v}}` (ett
//!   omärkt värde) eller `{"case": {"label": v, ...}}` (märkta värden);
//!   utan associerat värde: `{"case": {}}`.
//! - `Dictionary<UUID, Date>` kodas som en platt array `[k1, v1, k2, v2, ...]`,
//!   inte ett JSON-objekt (UUID är ingen giltig objektnyckel för Codable).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Sekunder sedan 2001-01-01T00:00:00Z — samma epok som Swifts
/// `Date.timeIntervalSinceReferenceDate`. Unix-epok ligger 978307200s tidigare.
const REFERENCE_DATE_UNIX_OFFSET: f64 = 978_307_200.0;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ReferenceDate(pub f64);

impl ReferenceDate {
    pub fn now() -> Self {
        let unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("systemklockan är före 1970")
            .as_secs_f64();
        ReferenceDate(unix - REFERENCE_DATE_UNIX_OFFSET)
    }
}

impl Serialize for ReferenceDate {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f64(self.0)
    }
}

impl<'de> Deserialize<'de> for ReferenceDate {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(ReferenceDate(f64::deserialize(d)?))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HostAuth {
    AskPassword,
    KeyFile(String),
    AgentDefault,
    KeychainKey(String),
    CertificateFile { key_path: String, cert_path: String },
    BitwardenItem(String),
}

impl Serialize for HostAuth {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut outer = s.serialize_map(Some(1))?;
        match self {
            HostAuth::AskPassword => {
                outer.serialize_entry("askPassword", &serde_json::json!({}))?
            }
            HostAuth::KeyFile(path) => {
                outer.serialize_entry("keyFile", &serde_json::json!({ "_0": path }))?
            }
            HostAuth::AgentDefault => {
                outer.serialize_entry("agentDefault", &serde_json::json!({}))?
            }
            HostAuth::KeychainKey(id) => {
                outer.serialize_entry("keychainKey", &serde_json::json!({ "_0": id }))?
            }
            HostAuth::CertificateFile {
                key_path,
                cert_path,
            } => outer.serialize_entry(
                "certificateFile",
                &serde_json::json!({ "keyPath": key_path, "certPath": cert_path }),
            )?,
            HostAuth::BitwardenItem(id) => {
                outer.serialize_entry("bitwardenItem", &serde_json::json!({ "_0": id }))?
            }
        }
        outer.end()
    }
}

impl<'de> Deserialize<'de> for HostAuth {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let map = HashMap::<String, serde_json::Value>::deserialize(d)?;
        if map.len() != 1 {
            return Err(D::Error::custom(format!(
                "HostAuth: väntade exakt en case-nyckel, hittade {}",
                map.len()
            )));
        }
        let (case, payload) = map
            .into_iter()
            .next()
            .ok_or_else(|| D::Error::custom("HostAuth: tom map, väntade exakt en case-nyckel"))?;
        let field = |name: &str| -> Result<String, D::Error> {
            payload
                .get(name)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or_else(|| D::Error::custom(format!("HostAuth::{case}: saknar fältet {name}")))
        };
        match case.as_str() {
            "askPassword" => Ok(HostAuth::AskPassword),
            "agentDefault" => Ok(HostAuth::AgentDefault),
            "keyFile" => Ok(HostAuth::KeyFile(field("_0")?)),
            "keychainKey" => Ok(HostAuth::KeychainKey(field("_0")?)),
            "bitwardenItem" => Ok(HostAuth::BitwardenItem(field("_0")?)),
            "certificateFile" => Ok(HostAuth::CertificateFile {
                key_path: field("keyPath")?,
                cert_path: field("certPath")?,
            }),
            other => Err(D::Error::custom(format!("HostAuth: okänd case {other}"))),
        }
    }
}

impl Default for HostAuth {
    fn default() -> Self {
        HostAuth::AgentDefault
    }
}

/// Motsvarar `RemotePlatform` — String-baserad rawValue-enum, kodas som en
/// vanlig JSON-sträng (till skillnad från `HostAuth`, som saknar rawValue).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemotePlatform {
    Posix,
    WindowsAdmin,
    WindowsStandard,
}

impl Default for RemotePlatform {
    fn default() -> Self {
        RemotePlatform::Posix
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub id: Uuid,
    pub alias: String,
    pub host_name: String,
    pub user: String,
    #[serde(default = "default_port")]
    pub port: i64,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub auth: HostAuth,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub color_tag: Option<String>,
    #[serde(default)]
    pub platform: RemotePlatform,
    #[serde(default)]
    pub startup_command: Option<String>,
    /// Stavningen är ett KONTRAKT mot Swift-sidan, inte en intern detalj.
    /// `rename_all = "camelCase"` skulle ge `jumpHostId`, men Swifts
    /// `CodingKeys` stavar den `jumpHostID` (Apples namnkonvention
    /// versaliserar initialförkortningar). Nycklarna matchade alltså inte,
    /// och `Codable`/serde släpper okända nycklar tyst — följden var att en
    /// ProxyJump-koppling FÖRSVANN i båda riktningarna så fort tillståndet
    /// synkades mellan en Linux- och en Apple-enhet. Värst tänkbara fält att
    /// tappa: målet är ofta bara nåbart genom hoppet, så anslutningen slutar
    /// fungera helt.
    ///
    /// `alias` läser den gamla stavningen så att redan sparade
    /// `hosts.json`-filer inte tappar sin koppling vid uppgraderingen.
    #[serde(default, rename = "jumpHostID", alias = "jumpHostId")]
    pub jump_host_id: Option<Uuid>,
    #[serde(default)]
    pub mac_address: Option<String>,
    /// Vidarebefordra den lokala ssh-agenten till värden (OpenSSH:s
    /// `ForwardAgent`).
    ///
    /// FALSKT som förval, och det är ett säkerhetsval snarare än ett
    /// bekvämlighetsval. Med agenten vidarebefordrad kan vem som helst
    /// med root på fjärrvärden använda DINA nycklar så länge sessionen
    /// lever — utan att kunna läsa dem, men utan att du märker något
    /// heller. OpenSSH har samma förval av samma skäl.
    #[serde(default)]
    pub forward_agent: bool,
    pub modified_at: ReferenceDate,
}

fn default_port() -> i64 {
    22
}

impl Host {
    pub fn new(alias: String, host_name: String, user: String) -> Self {
        Host {
            id: Uuid::new_v4(),
            alias,
            host_name,
            user,
            port: 22,
            tags: Vec::new(),
            auth: HostAuth::AgentDefault,
            is_favorite: false,
            color_tag: None,
            platform: RemotePlatform::Posix,
            startup_command: None,
            jump_host_id: None,
            mac_address: None,
            forward_agent: false,
            modified_at: ReferenceDate::now(),
        }
    }
}

/// Speglar `SyncState` — `tombstones` kodas platt (se modulnoten ovan), inte
/// som ett vanligt JSON-objekt.
#[derive(Debug, Clone, Default)]
pub struct SyncState {
    pub hosts: Vec<Host>,
    /// Synkade snippets. `#[serde(default)]` på avkodningssidan (se
    /// `Deserialize` nedan) — ett tillstånd skrivet innan snippets ingick i
    /// synken saknar fältet helt, och ska läsas som "inga snippets", inte
    /// avvisas. Samma bakåtkompatibilitet som Swift-sidans
    /// `decodeIfPresent`.
    pub snippets: Vec<crate::snippet::Snippet>,
    /// Delas av BÅDA posttyperna. UUID:n krockar inte mellan typerna, och
    /// en gemensam karta slipper ett andra fält på tråden.
    pub tombstones: HashMap<Uuid, ReferenceDate>,
}

impl Serialize for SyncState {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let flat: Vec<serde_json::Value> = self
            .tombstones
            .iter()
            .flat_map(|(k, v)| {
                [
                    serde_json::Value::String(k.to_string()),
                    serde_json::json!(v.0),
                ]
            })
            .collect();
        let mut st = s.serialize_struct("SyncState", 3)?;
        st.serialize_field("hosts", &self.hosts)?;
        st.serialize_field("snippets", &self.snippets)?;
        st.serialize_field("tombstones", &flat)?;
        st.end()
    }
}

impl<'de> Deserialize<'de> for SyncState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        #[derive(Deserialize)]
        struct Raw {
            hosts: Vec<Host>,
            #[serde(default)]
            snippets: Vec<crate::snippet::Snippet>,
            tombstones: Vec<serde_json::Value>,
        }
        let raw = Raw::deserialize(d)?;
        if raw.tombstones.len() % 2 != 0 {
            return Err(D::Error::custom(
                "tombstones: udda antal element i platt array",
            ));
        }
        let mut tombstones = HashMap::new();
        for pair in raw.tombstones.chunks_exact(2) {
            let id = pair[0]
                .as_str()
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| D::Error::custom("tombstones: ogiltig UUID-nyckel"))?;
            let date = pair[1]
                .as_f64()
                .ok_or_else(|| D::Error::custom("tombstones: ogiltigt datumvärde"))?;
            tombstones.insert(id, ReferenceDate(date));
        }
        Ok(SyncState {
            hosts: raw.hosts,
            snippets: raw.snippets,
            tombstones,
        })
    }
}

/// Persistent host-databas, `~/.bastion/hosts.json`. Motsvarar `HostStore.swift`
/// — samma fil kan läsas/skrivas av App/(iOS/macOS), Android och LinuxApp.
pub struct HostStore {
    path: std::path::PathBuf,
    state: SyncState,
}

impl HostStore {
    pub fn default_path() -> std::path::PathBuf {
        dirs::home_dir()
            .expect("kunde inte hitta hemkatalogen")
            .join(".bastion/hosts.json")
    }

    pub fn open(path: std::path::PathBuf) -> std::io::Result<Self> {
        let state = Self::load(&path)?;
        Ok(HostStore { path, state })
    }

    /// Skiljer "filen finns inte än" (tomt tillstånd är korrekt) från
    /// "filen finns men går inte att läsa/tolka" (propagera felet).
    /// Innan denna fix kollapsade båda till `SyncState::default()` — nästa
    /// `upsert`/`delete` skrev då permanent över en bara TRUNKERAD (inte
    /// tom) fil med ett tomt tillstånd, en tyst dataförlust utan varning.
    fn load(path: &std::path::Path) -> std::io::Result<SyncState> {
        let data = match std::fs::read_to_string(path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SyncState::default()),
            Err(e) => return Err(e),
        };
        // Nytt format (SyncState) först; annars äldre rena Host-listor.
        if let Ok(state) = serde_json::from_str::<SyncState>(&data) {
            return Ok(state);
        }
        if let Ok(hosts) = serde_json::from_str::<Vec<Host>>(&data) {
            return Ok(SyncState {
                hosts,
                snippets: Vec::new(),
                tombstones: HashMap::new(),
            });
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "{}: går inte att tolka som SyncState eller Vec<Host>",
                path.display()
            ),
        ))
    }

    pub fn all(&self) -> Vec<&Host> {
        let mut hosts: Vec<&Host> = self.state.hosts.iter().collect();
        hosts.sort_by_key(|h| h.alias.to_lowercase());
        hosts
    }

    pub fn upsert(&mut self, mut host: Host) -> std::io::Result<()> {
        host.modified_at = ReferenceDate::now();
        self.state.tombstones.remove(&host.id);
        if let Some(existing) = self.state.hosts.iter_mut().find(|h| h.id == host.id) {
            *existing = host;
        } else {
            self.state.hosts.push(host);
        }
        self.persist()
    }

    pub fn delete(&mut self, id: Uuid) -> std::io::Result<()> {
        self.state.hosts.retain(|h| h.id != id);
        self.state.tombstones.insert(id, ReferenceDate::now());
        self.persist()
    }

    /// Importerar värdar ur en `~/.ssh/config`-text — motsvarar
    /// `HostStore.importSSHConfig` i Swift. Alias som redan finns
    /// (skiftlägesokänsligt) hoppas över, så ett omimport av samma fil
    /// inte skapar dubbletter. Returnerar antalet FAKTISKT importerade.
    pub fn import_ssh_config(&mut self, text: &str) -> std::io::Result<usize> {
        self.import_parsed(crate::ssh_config::SSHConfig::parse(text))
    }

    /// Som `import_ssh_config`, men läser filen från disk och följer
    /// därmed `Include`-rader (se `ssh_config::SSHConfig::parse_file`).
    ///
    /// Skillnaden är inte kosmetisk: en config som bara består av
    /// `Include ~/.ssh/config.d/*` — hur 1Password, Colima, OrbStack m.fl.
    /// säger åt användaren att lägga upp sin — ger noll importerade värdar
    /// när bara texten klistras in, utan att något ser trasigt ut.
    pub fn import_ssh_config_file(&mut self, path: &std::path::Path) -> std::io::Result<usize> {
        self.import_parsed(crate::ssh_config::SSHConfig::parse_file(path)?)
    }

    fn import_parsed(&mut self, config: crate::ssh_config::SSHConfig) -> std::io::Result<usize> {
        let existing: std::collections::HashSet<String> =
            self.all().iter().map(|h| h.alias.to_lowercase()).collect();
        let fresh: Vec<Host> = crate::ssh_config::imported_hosts(&config)
            .into_iter()
            .filter(|h| !existing.contains(&h.alias.to_lowercase()))
            .collect();
        let count = fresh.len();
        let imported: Vec<(String, Option<String>)> = fresh
            .iter()
            .map(|h| {
                let jump = config.resolve(&h.alias).proxy_jump.as_deref()
                    .and_then(crate::ssh_config::proxy_jump_alias);
                (h.alias.clone(), jump)
            })
            .collect();
        for host in fresh {
            self.upsert(host)?;
        }

        // Andra pass: koppla ihop ProxyJump. Måste ske EFTER att alla
        // värdar sparats — jump-hosten kan stå efter den som pekar på den i
        // configen, och en koppling kräver mottagarens id.
        //
        // Bara nyimporterade värdar får sin jump satt. Att röra en värd som
        // redan fanns vore att tyst skriva om något användaren kan ha ställt
        // in själv, och hela importen bygger på att befintliga alias lämnas
        // ifred.
        let by_alias: std::collections::HashMap<String, Uuid> = self
            .all()
            .iter()
            .map(|h| (h.alias.to_lowercase(), h.id))
            .collect();
        for (alias, jump_alias) in imported {
            let Some(jump_alias) = jump_alias else { continue };
            let Some(&jump_id) = by_alias.get(&jump_alias.to_lowercase()) else { continue };
            let Some(&id) = by_alias.get(&alias.to_lowercase()) else { continue };
            // En värd som pekar på sig själv är ingen kedja, bara ett fel.
            if id == jump_id {
                continue;
            }
            // `all()` ger `Vec<&Host>`, så en `cloned()` på iteratorn ger
            // fortfarande en referens — klona värden explicit.
            let Some(mut host) = self.all().into_iter().find(|h| h.id == id).cloned() else {
                continue;
            };
            host.jump_host_id = Some(jump_id);
            self.upsert(host)?;
        }
        Ok(count)
    }

    /// Full synkrunda mot en transport: hämta fjärrtillstånd, slå ihop
    /// lokalt, skriv tillbaka det sammanslagna. Motsvarar
    /// `HostStore.sync(with:)` i Swift. Se `crate::sync`.
    ///
    /// Tar BÅDA databaserna, och det finns ingen variant som bara tar
    /// värdarna. Två synkvägar skulle betyda att någon förr eller senare
    /// väljer den som tyst hoppar över snippets. De delar dessutom
    /// gravstenskarta, så två separata rundturer vore inte bara långsammare
    /// utan fel: den andra pushen skulle skriva över den förstas gravstenar.
    pub fn sync_with_snippets(
        &mut self,
        provider: &impl crate::sync::SyncProvider,
        snippets: &mut crate::snippet::SnippetStore,
    ) -> std::io::Result<()> {
        let remote = provider.pull()?.unwrap_or_default();
        let local = SyncState {
            hosts: std::mem::take(&mut self.state.hosts),
            snippets: snippets.all().into_iter().cloned().collect(),
            tombstones: std::mem::take(&mut self.state.tombstones),
        };
        let merged = crate::sync::merge(local, remote);

        provider.push(&merged)?;
        snippets.replace_all(merged.snippets.clone())?;
        self.state = SyncState { snippets: Vec::new(), ..merged };
        self.persist()
    }

    /// Skriver en gravsten utan att röra värdlistan — för poster som lever i
    /// en annan databas men delar sync-tillstånd (se `SnippetStore`).
    pub fn record_tombstone(&mut self, id: Uuid) -> std::io::Result<()> {
        self.state.tombstones.insert(id, ReferenceDate::now());
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
        let json = serde_json::to_string_pretty(&self.state)?;
        crate::fsutil::atomic_write(&self.path, json.as_bytes())
    }

    /// Löser upp `host.jump_host_id` mot en riktig `Host` i den här
    /// databasen — motsvarar `AuthResolver.resolveConnectionPlan`s regel i
    /// Swift (`App/AuthResolver.swift`). Bara ETT hopp stöds: en jump-host
    /// som SJÄLV har en `jump_host_id` satt avvisas explicit istället för
    /// att tyst koppla genom sin egen jump och ignorera resten av kedjan —
    /// det vore en säkerhetsregression för den som medvetet satte upp en
    /// längre kedja (t.ex. redigerad i App/ efter att den här värden redan
    /// pekade på den). En jump-host-referens som pekar på ett ID som inte
    /// (längre) finns i databasen avvisas likaså — anslut ALDRIG direkt
    /// mot target och hoppa tyst över en konfigurerad jump-host bara för
    /// att den saknas (t.ex. inte hunnit synkas än).
    pub fn resolve_jump(&self, host: &Host) -> Result<Option<Host>, String> {
        let Some(jump_id) = host.jump_host_id else {
            return Ok(None);
        };
        let jump_host = self
            .all()
            .into_iter()
            .find(|h| h.id == jump_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "\"{}\" är inställd att ansluta via en jump-host som inte finns i databasen \
                     (borttagen, eller inte synkad än)",
                    host.alias
                )
            })?;
        if jump_host.jump_host_id.is_some() {
            return Err(format!(
                "jump-hosten \"{}\" har själv en jump-host satt — kedjor med mer än ett hopp \
                 stöds inte",
                jump_host.alias
            ));
        }
        Ok(Some(jump_host))
    }

    /// Värdar som är giltiga att välja som jump-host för `editing` — alltså
    /// exakt de `resolve_jump` ovan accepterar. Reglerna bor här, bredvid
    /// den som verkställer dem, så att host-dialogen inte kan råka erbjuda
    /// ett val som sedan avvisas vid anslutning.
    ///
    /// `editing` är `None` när en ny värd skapas (då finns inget eget ID att
    /// utesluta ännu).
    pub fn jump_host_candidates(&self, editing: Option<Uuid>) -> Vec<&Host> {
        self.all()
            .into_iter()
            // En värd kan inte gå via sig själv.
            .filter(|h| Some(h.id) != editing)
            // Bara ett hopp stöds — en värd som själv har en jump vore en kedja.
            .filter(|h| h.jump_host_id.is_none())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_auth_round_trips_and_matches_swift_wire_format() {
        let auth = HostAuth::KeyFile("/x/y".into());
        let json = serde_json::to_string(&auth).unwrap();
        assert_eq!(json, r#"{"keyFile":{"_0":"/x/y"}}"#);
        let back: HostAuth = serde_json::from_str(&json).unwrap();
        assert_eq!(back, auth);

        let json = serde_json::to_string(&HostAuth::AskPassword).unwrap();
        assert_eq!(json, r#"{"askPassword":{}}"#);

        let cert = HostAuth::CertificateFile {
            key_path: "/k".into(),
            cert_path: "/c".into(),
        };
        let json = serde_json::to_string(&cert).unwrap();
        let back: HostAuth = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cert);
    }

    #[test]
    fn reference_date_matches_swift_epoch() {
        // Verifierat mot en riktig `swift`-körning: Date(timeIntervalSinceReferenceDate: 0)
        // kodat via samma JSONEncoder gav modifiedAt = 0.
        let unix_epoch_as_reference = 0.0 - REFERENCE_DATE_UNIX_OFFSET;
        assert_eq!(unix_epoch_as_reference, -978_307_200.0);
    }

    #[test]
    fn sync_state_tombstones_are_flat_not_object() {
        let id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let mut tombstones = HashMap::new();
        tombstones.insert(id, ReferenceDate(5.0));
        let state = SyncState {
            hosts: vec![],
            snippets: Vec::new(),
            tombstones,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(
            json,
            r#"{"hosts":[],"snippets":[],"tombstones":["00000000-0000-0000-0000-000000000001",5.0]}"#
        );
        let back: SyncState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tombstones.len(), 1);
    }

    #[test]
    fn reads_a_hosts_json_actually_produced_by_swift() {
        let path = std::path::PathBuf::from("/tmp/swift-hosts.json");
        if !path.exists() {
            return; // bara körd manuellt mot en genererad fixture, se genhosts.swift
        }
        let store = HostStore::open(path).unwrap();
        let hosts = store.all();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "swift-genererad");
        assert_eq!(hosts[0].host_name, "10.0.0.5");
        assert_eq!(
            hosts[0].auth,
            HostAuth::KeyFile("/home/x/.ssh/id_ed25519".into())
        );
    }

    #[test]
    fn a_corrupt_hosts_json_is_an_error_not_a_silent_empty_state() {
        let dir = std::env::temp_dir().join(format!("bastion-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hosts.json");
        std::fs::write(&path, "{ det här är inte giltig json").unwrap();

        let result = HostStore::open(path);
        assert!(
            result.is_err(),
            "en trunkerad/skadad fil ska propagera ett fel, inte tyst bli SyncState::default()"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_missing_hosts_json_is_still_a_valid_empty_store() {
        let dir = std::env::temp_dir().join(format!("bastion-test-{}", Uuid::new_v4()));
        let path = dir.join("hosts.json");
        let store = HostStore::open(path).unwrap();
        assert!(store.all().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn host_auth_with_more_than_one_case_key_is_rejected() {
        let json = r#"{"askPassword":{},"agentDefault":{}}"#;
        let result: Result<HostAuth, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "en HostAuth med flera case-nycklar ska avvisas, inte godtyckligt välja en"
        );
    }

    #[test]
    fn host_store_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("bastion-test-{}", Uuid::new_v4()));
        let path = dir.join("hosts.json");
        let mut store = HostStore::open(path.clone()).unwrap();
        let host = Host::new("mp100".into(), "192.168.1.50".into(), "berduf".into());
        let id = host.id;
        store.upsert(host).unwrap();

        let reopened = HostStore::open(path).unwrap();
        assert_eq!(reopened.all().len(), 1);
        assert_eq!(reopened.all()[0].id, id);
        std::fs::remove_dir_all(dir).ok();
    }

    fn store_with(hosts: Vec<Host>) -> HostStore {
        let dir = std::env::temp_dir().join(format!("bastion-jump-test-{}", Uuid::new_v4()));
        let mut store = HostStore::open(dir.join("hosts.json")).unwrap();
        for h in hosts {
            store.upsert(h).unwrap();
        }
        store
    }

    #[test]
    fn resolve_jump_returns_none_when_no_jump_host_is_set() {
        let host = Host::new("direkt".into(), "1.2.3.4".into(), "u".into());
        let store = store_with(vec![host.clone()]);
        assert!(store.resolve_jump(&host).unwrap().is_none());
    }

    #[test]
    fn resolve_jump_finds_the_configured_jump_host() {
        let jump = Host::new("bastionvard".into(), "10.0.0.1".into(), "u".into());
        let mut target = Host::new("innanfor".into(), "192.168.1.5".into(), "u".into());
        target.jump_host_id = Some(jump.id);
        let store = store_with(vec![jump.clone(), target.clone()]);

        let resolved = store.resolve_jump(&target).unwrap();
        assert_eq!(resolved.unwrap().id, jump.id);
    }

    #[test]
    fn resolve_jump_rejects_a_missing_jump_host_instead_of_connecting_directly() {
        let mut target = Host::new("innanfor".into(), "192.168.1.5".into(), "u".into());
        target.jump_host_id = Some(Uuid::new_v4());
        let store = store_with(vec![target.clone()]);

        assert!(store.resolve_jump(&target).is_err());
    }

    #[test]
    fn resolve_jump_rejects_a_chained_jump_host() {
        let grandparent = Host::new("c".into(), "3.3.3.3".into(), "u".into());
        let mut parent = Host::new("b".into(), "2.2.2.2".into(), "u".into());
        parent.jump_host_id = Some(grandparent.id);
        let mut target = Host::new("a".into(), "1.1.1.1".into(), "u".into());
        target.jump_host_id = Some(parent.id);
        let store = store_with(vec![grandparent, parent, target.clone()]);

        let err = store.resolve_jump(&target).unwrap_err();
        assert!(
            err.contains("mer än ett hopp"),
            "felmeddelandet ska förklara VARFÖR, fick: {err}"
        );
    }

    #[test]
    fn jump_host_candidates_excludes_the_host_being_edited() {
        let a = Host::new("a".into(), "1.1.1.1".into(), "u".into());
        let b = Host::new("b".into(), "2.2.2.2".into(), "u".into());
        let store = store_with(vec![a.clone(), b.clone()]);

        let ids: Vec<_> = store.jump_host_candidates(Some(a.id)).iter().map(|h| h.id).collect();
        assert_eq!(ids, vec![b.id], "en värd får inte gå via sig själv");
    }

    #[test]
    fn jump_host_candidates_excludes_hosts_that_are_themselves_chained() {
        let plain = Host::new("plain".into(), "1.1.1.1".into(), "u".into());
        let mut chained = Host::new("chained".into(), "2.2.2.2".into(), "u".into());
        chained.jump_host_id = Some(plain.id);
        let store = store_with(vec![plain.clone(), chained]);

        let ids: Vec<_> = store.jump_host_candidates(None).iter().map(|h| h.id).collect();
        assert_eq!(
            ids,
            vec![plain.id],
            "bara ett hopp stöds, så en värd med egen jump får inte erbjudas"
        );
    }

    /// Kontraktet mellan dialogen och anslutningen: allt som erbjuds som val
    /// måste faktiskt gå att ansluta med. Annars flyttas felet från
    /// valögonblicket till anslutningsögonblicket, där det är svårare att
    /// förstå.
    #[test]
    fn every_offered_candidate_is_accepted_by_resolve_jump() {
        let plain = Host::new("plain".into(), "1.1.1.1".into(), "u".into());
        let other = Host::new("other".into(), "3.3.3.3".into(), "u".into());
        let mut chained = Host::new("chained".into(), "2.2.2.2".into(), "u".into());
        chained.jump_host_id = Some(plain.id);
        let mut target = Host::new("target".into(), "4.4.4.4".into(), "u".into());
        let store = store_with(vec![plain, other, chained, target.clone()]);

        let candidates: Vec<Uuid> = store
            .jump_host_candidates(Some(target.id))
            .iter()
            .map(|h| h.id)
            .collect();
        assert!(!candidates.is_empty(), "testet är meningslöst utan kandidater");

        for id in candidates {
            target.jump_host_id = Some(id);
            let resolved = store.resolve_jump(&target);
            assert!(
                resolved.is_ok(),
                "kandidaten {id} erbjöds i dialogen men avvisades vid anslutning: {:?}",
                resolved.err()
            );
        }
    }

    /// Fältnamnet i JSON är ett KONTRAKT mot Swift-sidan, inte en intern
    /// detalj: `Host` synkas mellan plattformarna som JSON, och en nyckel
    /// som inte stavas likadant på båda sidor släpps tyst av mottagaren.
    /// Precis det hände med `forwardAgent`, som saknades i Swift-modellen
    /// och därför raderades vid varje synk genom en Apple-enhet.
    #[test]
    fn forward_agent_is_serialised_under_the_name_the_other_platforms_expect() {
        let mut host = Host::new("h".into(), "10.0.0.1".into(), "a".into());
        host.forward_agent = true;
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&host).unwrap()).unwrap();
        assert_eq!(
            json.get("forwardAgent").and_then(|v| v.as_bool()),
            Some(true),
            "nyckeln måste heta forwardAgent — Swift-sidans CodingKeys stavar den så"
        );
    }

    /// Låser HELA JSON-formen för `Host`, inte bara ett fält.
    ///
    /// Två fältnamn har redan tyst gått isär mellan Rust och Swift
    /// (`forwardAgent` saknades helt på ena sidan, `jumpHostID` stavades
    /// `jumpHostId` på den andra), och båda gångerna var symptomet
    /// dataförlust vid synk utan ett enda felmeddelande. Ett test som bara
    /// kollar de fält man råkar tänka på hittar inte nästa. Det här kräver
    /// att nyckeluppsättningen är EXAKT den förväntade — lägger någon till
    /// ett fält faller testet, och då är frågan "har Swift-sidan samma
    /// stavning?" omöjlig att glömma.
    #[test]
    fn the_json_shape_of_host_is_exactly_what_the_other_platforms_expect() {
        let host = Host::new("h".into(), "10.0.0.1".into(), "a".into());
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&host).unwrap()).unwrap();
        let mut keys: Vec<String> = json.as_object().unwrap().keys().cloned().collect();
        keys.sort();

        let mut expected = vec![
            "alias", "auth", "colorTag", "forwardAgent", "hostName", "id", "isFavorite",
            "jumpHostID", "macAddress", "modifiedAt", "platform", "port", "startupCommand",
            "tags", "user",
        ];
        expected.sort();
        assert_eq!(
            keys, expected,
            "JSON-formen ändrades. Uppdatera Swift-sidans CodingKeys i samma veva, \
             annars släpps det nya fältet tyst vid synk."
        );
    }

    /// Guldfixtur som Swift-sidan avkodar från SAMMA fil, i
    /// `HostStoreTests.testTheSharedWireFormatFixtureDecodesIdentically`.
    ///
    /// Nyckeljämförelsen i testet ovan fångar att ett fält HETER samma sak.
    /// Den säger ingenting om nyttolastens FORM — att `HostAuth` bär sina
    /// fält som `{"certificateFile": {"keyPath": …}}` och att `platform` är
    /// en rå sträng, inte ett objekt. Går den formen isär tappas fältet lika
    /// tyst som ett felstavat nyckelnamn.
    ///
    /// Ändra aldrig fixturen för att få testet grönt. Faller den har
    /// trådformatet ändrats, och då är frågan vad som händer med redan
    /// sparade filer hos användarna.
    #[test]
    fn the_shared_wire_format_fixture_decodes_to_the_expected_host() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../Tests/fixtures/host-wire-format.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("kunde inte läsa {}: {e}", path.display()));
        let host: Host = serde_json::from_str(&text).expect("fixturen ska gå att avkoda");

        assert_eq!(host.id.to_string(), "6f1c9b2e-3a4d-4f57-8b91-0c2d5e7a1234");
        assert_eq!(host.alias, "guld");
        assert_eq!(host.host_name, "10.0.0.7");
        assert_eq!(host.user, "anders");
        assert_eq!(host.port, 2222);
        assert_eq!(host.tags, vec!["prod".to_string(), "eu".to_string()]);
        assert_eq!(
            host.auth,
            HostAuth::CertificateFile {
                key_path: "/home/anders/.ssh/id_ed25519".into(),
                cert_path: "/home/anders/.ssh/id_ed25519-cert.pub".into(),
            },
            "HostAuth-nyttolasten måste ha samma form som Swifts syntetiserade Codable"
        );
        assert!(host.is_favorite);
        assert_eq!(host.color_tag.as_deref(), Some("blue"));
        assert_eq!(host.platform, RemotePlatform::WindowsAdmin);
        assert_eq!(host.startup_command.as_deref(), Some("tmux attach"));
        assert_eq!(
            host.jump_host_id.map(|i| i.to_string()).as_deref(),
            Some("b7e4d1a0-8c33-4e29-9f6b-1d5a3c8e9876")
        );
        assert_eq!(host.mac_address.as_deref(), Some("AA:BB:CC:DD:EE:FF"));
        assert!(host.forward_agent);
        assert_eq!(host.modified_at.0, 800_000_000.0);

        // Och tillbaka: det vi skriver ska gå att läsa som samma sak igen.
        let round_tripped: Host =
            serde_json::from_str(&serde_json::to_string(&host).unwrap()).unwrap();
        assert_eq!(round_tripped.jump_host_id, host.jump_host_id);
        assert_eq!(round_tripped.auth, host.auth);
        assert_eq!(round_tripped.forward_agent, host.forward_agent);
    }

    /// Ett tillstånd skrivet INNAN snippets ingick i synken saknar fältet
    /// helt. Det ska läsas som "inga snippets", inte avvisas — annars slutar
    /// synken fungera för alla som inte uppgraderat alla sina enheter
    /// samtidigt, vilket ingen gör.
    #[test]
    fn a_sync_state_written_before_snippets_existed_still_loads() {
        let json = r#"{"hosts":[],"tombstones":[]}"#;
        let state: SyncState = serde_json::from_str(json).expect("gammalt tillstånd ska läsas");
        assert!(state.snippets.is_empty());
        assert!(state.hosts.is_empty());
    }

    /// `tombstones` är det ANDRA handskrivna kontraktet mot Swift-sidan.
    /// Swift kodar `[UUID: Date]` som en PLATT array av omväxlande nycklar
    /// och värden — inte som ett JSON-objekt — eftersom `UUID` inte är
    /// `CodingKeyRepresentable`. Rust-sidans egna `Serialize`/`Deserialize`
    /// finns bara för att härma det.
    ///
    /// Går formen isär tappas gravstenarna, och en gravsten som tappas
    /// betyder att en RADERAD värd återuppstår vid nästa synk. Det är den
    /// sortens fel som ser ut som ett spöke i gränssnittet och aldrig som
    /// ett serialiseringsproblem.
    ///
    /// Swift-sidan läser samma fil i
    /// `SyncEngineTests.testTheSharedSyncStateFixtureDecodesIdentically`.
    #[test]
    fn the_shared_sync_state_fixture_decodes_to_the_expected_state() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../Tests/fixtures/sync-state-wire-format.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("kunde inte läsa {}: {e}", path.display()));
        let state: SyncState = serde_json::from_str(&text).expect("fixturen ska gå att avkoda");

        assert_eq!(state.hosts.len(), 1);
        assert_eq!(state.hosts[0].alias, "synk");
        assert_eq!(state.snippets.len(), 1, "snippets ingår i tillståndet");
        assert_eq!(state.snippets[0].name, "starta om plex");
        assert_eq!(state.snippets[0].template, "docker compose restart {{tjanst}}");
        assert_eq!(state.snippets[0].modified_at.0, 787_000_000.0);
        assert_eq!(state.tombstones.len(), 2, "den platta arrayen ska ge TVÅ gravstenar");

        let first = Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap();
        let second = Uuid::parse_str("99999999-8888-7777-6666-555555555555").unwrap();
        assert_eq!(state.tombstones.get(&first).map(|d| d.0), Some(780_000_000.0));
        assert_eq!(state.tombstones.get(&second).map(|d| d.0), Some(785_000_000.0));

        // Det vi SKRIVER måste ha samma platta form, annars kan Swift-sidan
        // inte läsa det vi just bevisat att vi kan läsa.
        let written: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        let flat = written["tombstones"].as_array().expect("tombstones ska vara en array");
        assert_eq!(flat.len(), 4, "två gravstenar = fyra element, inte ett objekt");
        assert!(flat[0].is_string() && flat[1].is_number(), "nyckel, värde, nyckel, värde");

        let round_tripped: SyncState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(round_tripped.tombstones.len(), 2);
    }

    /// En udda array är trasig data, inte "sista gravstenen utan datum" —
    /// att tyst slänga den sista skulle återuppliva en raderad värd.
    #[test]
    fn an_odd_length_tombstone_array_is_an_error_not_a_silent_truncation() {
        let json = r#"{"hosts":[],"tombstones":["aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"]}"#;
        assert!(
            serde_json::from_str::<SyncState>(json).is_err(),
            "udda antal element måste vara ett fel"
        );
    }

    /// En `hosts.json` skriven före namnbytet ska INTE tappa sin
    /// ProxyJump-koppling vid uppgraderingen.
    #[test]
    fn the_old_jump_host_spelling_is_still_read() {
        let id = Uuid::new_v4();
        let json = format!(
            r#"{{"id":"{}","alias":"a","hostName":"h","user":"u","port":22,"tags":[],
                 "auth":{{"agentDefault":{{}}}},"modifiedAt":789000000.0,
                 "jumpHostId":"{}"}}"#,
            Uuid::new_v4(),
            id
        );
        let host: Host = serde_json::from_str(&json).expect("gammal fil ska gå att läsa");
        assert_eq!(host.jump_host_id, Some(id), "den gamla stavningen måste fortfarande läsas");
    }
}
