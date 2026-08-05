//! Port av `Sources/SSHCore/WireGuardConfig.swift` +
//! `WireGuardProfileStore.swift`. v1: parsning/lagring/redigering av
//! WireGuard-profiler (`.conf`-filen `wg-quick`/`wg setconf` läser) — INTE
//! att faktiskt upprätta tunneln (kräver `wg`-binären + root, eller ett
//! helt eget kryptoprotokoll om det ska göras utan den binären — separat,
//! mycket större arbete, se ROADMAP.md "Native WireGuard/Tailscale").
//!
//! Formatet verifierat mot `wg(8)`/`wg-quick(8)` av Swift-sidan (inte
//! omgissat här): `[Interface]`-sektionen bär `PrivateKey`/`ListenPort`/
//! `FwMark` (wg(8)) samt `Address`/`DNS`/`MTU`/`Table`/`PreUp`/`PostUp`/
//! `PreDown`/`PostDown`/`SaveConfig` (wg-quick-tillägg); `[Peer]`-sektionen
//! bär `PublicKey`/`PresharedKey`/`AllowedIPs`/`Endpoint`/`PersistentKeepalive`.

use crate::host::ReferenceDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    pub private_key: Option<String>,
    #[serde(default)]
    pub address: Vec<String>,
    #[serde(default)]
    pub dns: Vec<String>,
    pub listen_port: Option<i64>,
    pub mtu: Option<i64>,
    pub table: Option<String>,
    #[serde(default)]
    pub pre_up: Vec<String>,
    #[serde(default)]
    pub post_up: Vec<String>,
    #[serde(default)]
    pub pre_down: Vec<String>,
    #[serde(default)]
    pub post_down: Vec<String>,
    pub save_config: Option<bool>,
    pub fw_mark: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub public_key: Option<String>,
    pub preshared_key: Option<String>,
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    pub endpoint: Option<String>,
    pub persistent_keepalive: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WireGuardConfig {
    #[serde(default)]
    pub interface: Interface,
    #[serde(default)]
    pub peers: Vec<Peer>,
}

enum Section {
    None,
    Interface,
    Peer,
}

impl WireGuardConfig {
    /// `#` inleder en kommentar (till radslutet), `[Section]`-rubriker,
    /// `Key = Value`-par — nycklar skiftlägesokänsliga (verkliga `.conf`-
    /// filer varierar), värden trimmas. Kommaseparerade listor
    /// (`Address`/`DNS`/`AllowedIPs`) delas och trimmas per element. En
    /// nyckel som upprepas (t.ex. flera `Address`-rader, tillåtet enligt
    /// wg-quick) ackumuleras istället för att skriva över.
    pub fn parse(text: &str) -> WireGuardConfig {
        let mut iface = Interface::default();
        let mut peer_list: Vec<Peer> = Vec::new();
        let mut current_peer: Option<Peer> = None;
        let mut section = Section::None;

        fn flush_peer(peer_list: &mut Vec<Peer>, current_peer: &mut Option<Peer>) {
            if let Some(p) = current_peer.take() {
                peer_list.push(p);
            }
        }

        fn comma_list(s: &str) -> Vec<String> {
            s.split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        }

        for raw_line in text.split(['\n', '\r']) {
            // En rad som BÖRJAR med "#" (en helt utkommenterad rad, t.ex.
            // tillfälligt avstängd "#Address = …") ska ge en TOM sträng
            // före "#", inte tappa den — samma fälla som Swift-sidans
            // CodeRabbit-fynd (PR #79) dokumenterar för
            // `split(separator:maxSplits:omittingEmptySubsequences:)`.
            // Rusts `str::split` behåller redan tomma segment (till
            // skillnad från Swifts `omittingEmptySubsequences: true`-
            // standard), så `.next()` här ger korrekt en tom sträng för
            // just det fallet — testat explicit, se
            // `leading_hash_comment_line_is_ignored_not_parsed_as_active_config`.
            let without_comment = raw_line.split('#').next().unwrap_or("");
            let line = without_comment.trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                let name = line[1..line.len() - 1].trim().to_lowercase();
                flush_peer(&mut peer_list, &mut current_peer);
                if name == "peer" {
                    current_peer = Some(Peer::default());
                    section = Section::Peer;
                } else if name == "interface" {
                    section = Section::Interface;
                } else {
                    section = Section::None;
                }
                continue;
            }

            let Some(eq) = line.find('=') else { continue };
            let key = line[..eq].trim().to_lowercase();
            let value = line[eq + 1..].trim().to_string();
            if key.is_empty() {
                continue;
            }

            match section {
                Section::Interface => match key.as_str() {
                    "privatekey" => iface.private_key = Some(value),
                    "address" => iface.address.extend(comma_list(&value)),
                    "dns" => iface.dns.extend(comma_list(&value)),
                    "listenport" => iface.listen_port = value.parse().ok(),
                    "mtu" => iface.mtu = value.parse().ok(),
                    "table" => iface.table = Some(value),
                    "preup" => iface.pre_up.push(value),
                    "postup" => iface.post_up.push(value),
                    "predown" => iface.pre_down.push(value),
                    "postdown" => iface.post_down.push(value),
                    "saveconfig" => iface.save_config = Some(value.to_lowercase() == "true"),
                    "fwmark" => iface.fw_mark = Some(value),
                    _ => {}
                },
                Section::Peer => {
                    if let Some(p) = current_peer.as_mut() {
                        match key.as_str() {
                            "publickey" => p.public_key = Some(value),
                            "presharedkey" => p.preshared_key = Some(value),
                            "allowedips" => p.allowed_ips.extend(comma_list(&value)),
                            "endpoint" => p.endpoint = Some(value),
                            "persistentkeepalive" => p.persistent_keepalive = value.parse().ok(),
                            _ => {}
                        }
                    }
                }
                Section::None => {}
            }
        }
        flush_peer(&mut peer_list, &mut current_peer);
        WireGuardConfig {
            interface: iface,
            peers: peer_list,
        }
    }

    /// Skriver tillbaka till `.conf`-textformat — inversen av `parse`.
    /// Fältordningen matchar `wg-quick`s egen konvention (Interface-nycklar
    /// i samma ordning som `wg-quick(8)` listar dem, sedan en `[Peer]`-
    /// sektion per peer).
    pub fn rendered(&self) -> String {
        let mut lines: Vec<String> = vec!["[Interface]".to_string()];
        if let Some(v) = &self.interface.private_key {
            lines.push(format!("PrivateKey = {v}"));
        }
        if !self.interface.address.is_empty() {
            lines.push(format!("Address = {}", self.interface.address.join(", ")));
        }
        if !self.interface.dns.is_empty() {
            lines.push(format!("DNS = {}", self.interface.dns.join(", ")));
        }
        if let Some(v) = self.interface.listen_port {
            lines.push(format!("ListenPort = {v}"));
        }
        if let Some(v) = self.interface.mtu {
            lines.push(format!("MTU = {v}"));
        }
        if let Some(v) = &self.interface.table {
            lines.push(format!("Table = {v}"));
        }
        for v in &self.interface.pre_up {
            lines.push(format!("PreUp = {v}"));
        }
        for v in &self.interface.post_up {
            lines.push(format!("PostUp = {v}"));
        }
        for v in &self.interface.pre_down {
            lines.push(format!("PreDown = {v}"));
        }
        for v in &self.interface.post_down {
            lines.push(format!("PostDown = {v}"));
        }
        if let Some(v) = self.interface.save_config {
            lines.push(format!("SaveConfig = {}", if v { "true" } else { "false" }));
        }
        if let Some(v) = &self.interface.fw_mark {
            lines.push(format!("FwMark = {v}"));
        }

        for peer in &self.peers {
            lines.push(String::new());
            lines.push("[Peer]".to_string());
            if let Some(v) = &peer.public_key {
                lines.push(format!("PublicKey = {v}"));
            }
            if let Some(v) = &peer.preshared_key {
                lines.push(format!("PresharedKey = {v}"));
            }
            if !peer.allowed_ips.is_empty() {
                lines.push(format!("AllowedIPs = {}", peer.allowed_ips.join(", ")));
            }
            if let Some(v) = &peer.endpoint {
                lines.push(format!("Endpoint = {v}"));
            }
            if let Some(v) = peer.persistent_keepalive {
                lines.push(format!("PersistentKeepalive = {v}"));
            }
        }
        lines.join("\n") + "\n"
    }
}

/// Ett namngivet, sparat `WireGuardConfig` — samma "wrapper runt ren
/// datamodell"-mönster som `Snippet` runt sin `template`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireGuardProfile {
    pub id: Uuid,
    pub name: String,
    pub config: WireGuardConfig,
    pub modified_at: ReferenceDate,
}

impl WireGuardProfile {
    pub fn new(name: String, config: WireGuardConfig) -> Self {
        WireGuardProfile {
            id: Uuid::new_v4(),
            name,
            config,
            modified_at: ReferenceDate::now(),
        }
    }
}

/// Persistent WireGuard-profildatabas, `~/.bastion/wireguard.json` — samma
/// mönster som `SnippetStore`.
pub struct WireGuardProfileStore {
    path: std::path::PathBuf,
    profiles: Vec<WireGuardProfile>,
}

impl WireGuardProfileStore {
    pub fn default_path() -> std::path::PathBuf {
        dirs::home_dir()
            .expect("kunde inte hitta hemkatalogen")
            .join(".bastion/wireguard.json")
    }

    pub fn open(path: std::path::PathBuf) -> std::io::Result<Self> {
        let profiles = match std::fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{}: {e}", path.display()),
                )
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(WireGuardProfileStore { path, profiles })
    }

    pub fn all(&self) -> Vec<&WireGuardProfile> {
        let mut p: Vec<&WireGuardProfile> = self.profiles.iter().collect();
        p.sort_by_key(|x| x.name.to_lowercase());
        p
    }

    /// Inte anropad av UI:t än (raderna i `main.rs` klonar direkt ur
    /// `all()` istället) — kvar för symmetri med resten av store-API:t
    /// (`HostStore`/`SnippetStore` har motsvarande) och testad direkt.
    #[allow(dead_code)]
    pub fn get(&self, id: Uuid) -> Option<&WireGuardProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn upsert(&mut self, mut profile: WireGuardProfile) -> std::io::Result<()> {
        profile.modified_at = ReferenceDate::now();
        if let Some(existing) = self.profiles.iter_mut().find(|p| p.id == profile.id) {
            *existing = profile;
        } else {
            self.profiles.push(profile);
        }
        self.persist()
    }

    pub fn delete(&mut self, id: Uuid) -> std::io::Result<()> {
        self.profiles.retain(|p| p.id != id);
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
        let mut sorted = self.profiles.clone();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        crate::fsutil::atomic_write(&self.path, serde_json::to_string_pretty(&sorted)?.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# min hem-VPN
[Interface]
PrivateKey = safe-test-private-key
Address = 10.0.0.2/24, fd00::2/64
DNS = 1.1.1.1, home.example.com
ListenPort = 51820
MTU = 1420
Table = auto
PostUp = iptables -A FORWARD -i %i -j ACCEPT
PostDown = iptables -D FORWARD -i %i -j ACCEPT
SaveConfig = true

[Peer]
# servern hemma
PublicKey = HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=
PresharedKey = safe-test-preshared-key
AllowedIPs = 0.0.0.0/0, ::/0
Endpoint = vpn.example.com:51820
PersistentKeepalive = 25

[PEER]
PublicKey = anotherKeyBase64Placeholder1234567890abcdef=
AllowedIPs = 10.0.0.3/32
";

    #[test]
    fn parses_interface_fields() {
        let config = WireGuardConfig::parse(SAMPLE);
        assert_eq!(config.interface.private_key.as_deref(), Some("safe-test-private-key"));
        assert_eq!(config.interface.address, vec!["10.0.0.2/24", "fd00::2/64"]);
        assert_eq!(config.interface.dns, vec!["1.1.1.1", "home.example.com"]);
        assert_eq!(config.interface.listen_port, Some(51820));
        assert_eq!(config.interface.mtu, Some(1420));
        assert_eq!(config.interface.table.as_deref(), Some("auto"));
        assert_eq!(config.interface.post_up, vec!["iptables -A FORWARD -i %i -j ACCEPT"]);
        assert_eq!(config.interface.post_down, vec!["iptables -D FORWARD -i %i -j ACCEPT"]);
        assert_eq!(config.interface.save_config, Some(true));
    }

    #[test]
    fn parses_multiple_peers_including_case_insensitive_section_header() {
        let config = WireGuardConfig::parse(SAMPLE);
        assert_eq!(config.peers.len(), 2);
        assert_eq!(config.peers[0].public_key.as_deref(), Some("HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw="));
        assert_eq!(config.peers[0].preshared_key.as_deref(), Some("safe-test-preshared-key"));
        assert_eq!(config.peers[0].allowed_ips, vec!["0.0.0.0/0", "::/0"]);
        assert_eq!(config.peers[0].endpoint.as_deref(), Some("vpn.example.com:51820"));
        assert_eq!(config.peers[0].persistent_keepalive, Some(25));
        // "[PEER]" (versaler) ska tolkas likadant som "[Peer]".
        assert_eq!(config.peers[1].public_key.as_deref(), Some("anotherKeyBase64Placeholder1234567890abcdef="));
        assert_eq!(config.peers[1].allowed_ips, vec!["10.0.0.3/32"]);
    }

    #[test]
    fn comments_are_stripped() {
        let parsed = WireGuardConfig::parse("[Interface]\nPrivateKey = abc123= # min nyckel\n");
        assert_eq!(parsed.interface.private_key.as_deref(), Some("abc123="));
    }

    /// En rad som BÖRJAR med "#" (en helt utkommenterad nyckel) ska
    /// ignoreras helt, inte tolkas som aktiv config för texten efter "#" —
    /// samma CodeRabbit-fynd som Swift-sidan (PR #79) vaktar mot.
    #[test]
    fn leading_hash_comment_line_is_ignored_not_parsed_as_active_config() {
        let config = WireGuardConfig::parse("[Interface]\n#Address = 10.0.0.99/32\nPrivateKey = x\n");
        assert!(config.interface.address.is_empty());
        assert_eq!(config.interface.private_key.as_deref(), Some("x"));
    }

    #[test]
    fn round_trip_through_rendered_preserves_all_fields() {
        let original = WireGuardConfig::parse(SAMPLE);
        let rerendered = WireGuardConfig::parse(&original.rendered());
        assert_eq!(original, rerendered);
    }

    #[test]
    fn empty_config_renders_only_interface_header() {
        let config = WireGuardConfig::default();
        assert_eq!(config.rendered(), "[Interface]\n");
    }

    #[test]
    fn missing_optional_fields_stay_none() {
        let config = WireGuardConfig::parse("[Interface]\nPrivateKey = abc=\n");
        assert!(config.interface.listen_port.is_none());
        assert!(config.interface.mtu.is_none());
        assert!(config.interface.save_config.is_none());
        assert!(config.interface.address.is_empty());
        assert!(config.peers.is_empty());
    }

    #[test]
    fn repeated_address_lines_accumulate_instead_of_overwriting() {
        let config = WireGuardConfig::parse("[Interface]\nAddress = 10.0.0.2/24\nAddress = fd00::2/64\n");
        assert_eq!(config.interface.address, vec!["10.0.0.2/24", "fd00::2/64"]);
    }

    #[test]
    fn save_config_false() {
        let config = WireGuardConfig::parse("[Interface]\nSaveConfig = false\n");
        assert_eq!(config.interface.save_config, Some(false));
    }

    #[test]
    fn peer_without_preceding_interface_section_is_ignored() {
        // Nycklar innan någon sektionsrubrik hör inte hemma någonstans —
        // ska ignoreras tyst, inte krascha eller hamna fel.
        let config = WireGuardConfig::parse("PrivateKey = orphan=\n[Peer]\nPublicKey = pk=\n");
        assert!(config.interface.private_key.is_none());
        assert_eq!(config.peers.first().and_then(|p| p.public_key.as_deref()), Some("pk="));
    }

    fn make_config(private_key: &str) -> WireGuardConfig {
        let mut config = WireGuardConfig::default();
        config.interface.private_key = Some(private_key.to_string());
        config.interface.address = vec!["10.0.0.2/24".to_string()];
        config.peers = vec![Peer {
            public_key: Some("pk=".to_string()),
            allowed_ips: vec!["0.0.0.0/0".to_string()],
            ..Default::default()
        }];
        config
    }

    #[test]
    fn store_upsert_get_delete_sorted() {
        let dir = std::env::temp_dir().join(format!("bastion-wg-test-{}", Uuid::new_v4()));
        let mut store = WireGuardProfileStore::open(dir.join("wireguard.json")).unwrap();
        let home = WireGuardProfile::new("Hemma".to_string(), make_config("a="));
        let work = WireGuardProfile::new("jobbet".to_string(), make_config("b="));
        let home_id = home.id;
        let work_id = work.id;
        store.upsert(home).unwrap();
        store.upsert(work).unwrap();

        let names: Vec<&str> = store.all().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Hemma", "jobbet"]); // skiftlägesokänslig sort
        assert_eq!(store.get(home_id).unwrap().config.interface.private_key.as_deref(), Some("a="));

        store.delete(work_id).unwrap();
        let names: Vec<&str> = store.all().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Hemma"]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn store_persists_across_instances() {
        let dir = std::env::temp_dir().join(format!("bastion-wg-test-{}", Uuid::new_v4()));
        let path = dir.join("wireguard.json");
        let profile = WireGuardProfile::new("Hemma".to_string(), make_config("a="));
        let id = profile.id;
        {
            let mut s1 = WireGuardProfileStore::open(path.clone()).unwrap();
            s1.upsert(profile).unwrap();
        }
        let s2 = WireGuardProfileStore::open(path).unwrap();
        assert_eq!(s2.get(id).unwrap().name, "Hemma");
        std::fs::remove_dir_all(dir).ok();
    }

    /// Bevisar hela vägen: text -> WireGuardConfig -> WireGuardProfile ->
    /// lagrad JSON -> ny store-instans -> tillbaka till .conf-text, allt
    /// identiskt med originalet.
    #[test]
    fn full_round_trip_through_store_and_back_to_conf_text() {
        let text = "[Interface]\nPrivateKey = safe-test-private-key\nAddress = 10.0.0.2/24\n\n[Peer]\nPublicKey = HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=\nAllowedIPs = 0.0.0.0/0\nEndpoint = vpn.example.com:51820\n";
        let config = WireGuardConfig::parse(text);
        let profile = WireGuardProfile::new("Hemma".to_string(), config.clone());
        let id = profile.id;

        let dir = std::env::temp_dir().join(format!("bastion-wg-roundtrip-{}", Uuid::new_v4()));
        let path = dir.join("wireguard.json");
        let mut s1 = WireGuardProfileStore::open(path.clone()).unwrap();
        s1.upsert(profile).unwrap();

        let s2 = WireGuardProfileStore::open(path).unwrap();
        let reloaded = s2.get(id).unwrap();
        assert_eq!(reloaded.config, config);
        assert_eq!(reloaded.config.rendered(), config.rendered());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_corrupt_wireguard_json_is_an_error_not_a_silent_empty_state() {
        let dir = std::env::temp_dir().join(format!("bastion-wg-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wireguard.json");
        std::fs::write(&path, "{ inte giltig json").unwrap();

        let result = WireGuardProfileStore::open(path);
        assert!(
            result.is_err(),
            "en trunkerad/skadad fil ska propagera ett fel, inte tyst bli en tom lista"
        );
        std::fs::remove_dir_all(dir).ok();
    }
}
