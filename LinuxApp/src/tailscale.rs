//! Parsning av `tailscale status --json` — för att föreslå värdar ur
//! användarens tailnet (samma idé som ssh-config-import, men källan är
//! Tailscales egen lokala daemon istället för en textfil). Port av
//! `Sources/SSHCore/TailscaleStatus.swift`.
//!
//! **Viktig begränsning, medvetet** (samma som Swift-sidan): Tailscale
//! dokumenterar INTE det här JSON-formatet som en stabil, garanterad
//! kontraktsyta — bara att det är tänkt för automatisering. Fältnamnen är
//! samma som redan verifierats mot en RIKTIG, lokalt installerad
//! `tailscaled` av Swift-sidan (se dess doc-kommentar) — inte omgissade
//! här.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PeerInfo {
    #[serde(rename = "HostName")]
    pub host_name: String,
    #[serde(rename = "DNSName", default)]
    pub dns_name: String,
    #[serde(rename = "OS", default)]
    pub os: String,
    #[serde(rename = "TailscaleIPs")]
    pub tailscale_ips: Option<Vec<String>>,
    #[serde(rename = "Online", default)]
    pub online: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TailscaleStatus {
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "BackendState")]
    pub backend_state: String,
    #[serde(rename = "Self")]
    pub self_node: Option<PeerInfo>,
    #[serde(rename = "Peer")]
    pub peer: Option<std::collections::HashMap<String, PeerInfo>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailscaleError {
    /// Lokal `tailscale status --json`-körning gav en icke-noll exitkod.
    LocalCommandFailed { exit_code: i32, stderr: String },
    /// Fjärrkörning (över SSH) gav ett fel — se `ssh::run_command`s
    /// felmeddelande.
    RemoteCommandFailed(String),
    ParseFailed(String),
    Io(String),
}

impl std::fmt::Display for TailscaleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TailscaleError::LocalCommandFailed { exit_code, stderr } => {
                write!(f, "tailscale status misslyckades (kod {exit_code}): {stderr}")
            }
            TailscaleError::RemoteCommandFailed(e) => write!(f, "{e}"),
            TailscaleError::ParseFailed(e) => write!(f, "kunde inte tolka tailscale-utdata: {e}"),
            TailscaleError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl TailscaleStatus {
    pub fn parse(json: &str) -> Result<TailscaleStatus, TailscaleError> {
        serde_json::from_str(json).map_err(|e| TailscaleError::ParseFailed(e.to_string()))
    }

    /// Föreslagna värdar ur tailnet — bara peers som faktiskt är online och
    /// har minst en Tailscale-IP, sorterade på värdnamn. Föredrar `DNSName`
    /// (MagicDNS, t.ex. `min-server.tailXXXX.ts.net`) framför det korta
    /// `HostName` när MagicDNS är aktiverat — men faller tillbaka till
    /// `host_name` om `dns_name` saknas (peer utan MagicDNS, eller en äldre
    /// Tailscale-version).
    pub fn suggested_hosts(&self) -> Vec<(String, String)> {
        let mut suggestions: Vec<(String, String)> = self
            .peer
            .as_ref()
            .map(|p| p.values().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter(|info| info.online)
            .filter_map(|info| {
                let ip = info.tailscale_ips.as_ref()?.first()?.clone();
                let name = if info.dns_name.is_empty() {
                    info.host_name.clone()
                } else {
                    info.dns_name.trim_end_matches('.').to_string()
                };
                Some((name, ip))
            })
            .collect();
        suggestions.sort_by_key(|a| a.0.to_lowercase());
        suggestions
    }
}

/// Kör `tailscale status --json` LOKALT på maskinen appen själv exekverar
/// på — samma idé som ssh-config-import läser en lokal fil, men källan här
/// är Tailscales egen lokala daemon. Föreslår DENNA maskins tailnet-peers.
/// `program`/`args` injicerbara (inte hårdkodat `tailscale`) så tester kan
/// peka på ett riktigt, kortlivat skript istället för att mocka bort själva
/// processkörningen — samma "verifiera mot en riktig process"-princip som
/// Swift-sidan.
pub fn fetch_local(program: &str, args: &[&str]) -> Result<TailscaleStatus, TailscaleError> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| TailscaleError::Io(e.to_string()))?;
    if !output.status.success() {
        return Err(TailscaleError::LocalCommandFailed {
            exit_code: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    TailscaleStatus::parse(&stdout)
}

/// Startar `fetch_local` på en egen bakgrundstråd — `fetch_local` blockerar
/// (`std::process::Command::output` väntar in hela processen), och GTK:s
/// huvudloop får aldrig blockeras (samma resonemang som Swift-sidans
/// `Task.detached`-kommentar i `TailscaleDiscoveryModel.fetch`, som
/// specifikt flaggar att en direkt, oflyttad `fetchLocal()`-körning hade
/// frusit hela dialogen).
pub fn spawn_fetch_local() -> async_channel::Receiver<Result<TailscaleStatus, TailscaleError>> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let result = fetch_local("tailscale", &["status", "--json"]);
        let _ = tx.send_blocking(result);
    });
    rx
}

/// Kör `tailscale status --json` via SSH på en redan konfigurerad
/// fjärrvärd — samma anslutningsväg (inklusive jump-host) som allt annat
/// engångskommando i appen.
pub async fn fetch_remote(
    host: crate::host::Host,
    password: Option<String>,
    jump: Option<crate::host::Host>,
) -> Result<TailscaleStatus, TailscaleError> {
    let rx = crate::ssh::run_command(
        host,
        password,
        "tailscale status --json 2>/dev/null".to_string(),
        jump,
    );
    let output = rx
        .recv()
        .await
        .map_err(|_| TailscaleError::RemoteCommandFailed("kanalen stängdes oväntat".to_string()))?
        .map_err(TailscaleError::RemoteCommandFailed)?;
    TailscaleStatus::parse(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Riktig `tailscale status --json`-utskrift, samma fixtur som
    /// Swift-sidans `TailscaleStatusTests` (fångad från en genuint
    /// installerad `tailscaled`) — inte handskriven.
    const REAL_NO_LOGIN_JSON: &str = r#"{
      "Version": "1.98.8-t1241b225b-g0520dfda5",
      "TUN": true,
      "BackendState": "NeedsLogin",
      "AuthURL": "",
      "TailscaleIPs": null,
      "Self": {
        "ID": "",
        "PublicKey": "nodekey:0000000000000000000000000000000000000000000000000000000000000000",
        "HostName": "mp100",
        "DNSName": "",
        "OS": "linux",
        "UserID": 0,
        "TailscaleIPs": null,
        "Online": false
      },
      "Health": ["Tailscale is stopped."],
      "MagicDNSSuffix": "",
      "CurrentTailnet": null,
      "CertDomains": null,
      "Peer": null,
      "User": null,
      "ClientVersion": null
    }"#;

    const WITH_PEERS_JSON: &str = r#"{
      "Version": "1.98.8-t1241b225b-g0520dfda5",
      "BackendState": "Running",
      "Self": {"HostName": "mp100", "DNSName": "mp100.tail1234.ts.net.", "OS": "linux", "TailscaleIPs": ["100.64.0.1"], "Online": true},
      "Peer": {
        "nodekey:aaa": {"HostName": "nas", "DNSName": "nas.tail1234.ts.net.", "OS": "linux", "TailscaleIPs": ["100.64.0.2"], "Online": true},
        "nodekey:bbb": {"HostName": "laptop", "DNSName": "", "OS": "macOS", "TailscaleIPs": ["100.64.0.3"], "Online": false}
      }
    }"#;

    #[test]
    fn parses_real_needs_login_status() {
        let status = TailscaleStatus::parse(REAL_NO_LOGIN_JSON).unwrap();
        assert_eq!(status.version, "1.98.8-t1241b225b-g0520dfda5");
        assert_eq!(status.backend_state, "NeedsLogin");
        assert_eq!(status.self_node.unwrap().host_name, "mp100");
        assert!(status.peer.is_none());
    }

    #[test]
    fn suggested_hosts_empty_without_peers() {
        let status = TailscaleStatus::parse(REAL_NO_LOGIN_JSON).unwrap();
        assert!(status.suggested_hosts().is_empty());
    }

    #[test]
    fn suggested_hosts_only_includes_online_peers_with_an_ip() {
        let status = TailscaleStatus::parse(WITH_PEERS_JSON).unwrap();
        let suggested = status.suggested_hosts();
        assert_eq!(suggested.len(), 1);
        assert_eq!(suggested[0].0, "nas.tail1234.ts.net");
        assert_eq!(suggested[0].1, "100.64.0.2");
    }

    // MARK: - fetch_local (riktig, kortlivad process — inte mockad)

    #[test]
    fn fetch_local_parses_real_process_output() {
        let status = fetch_local(
            "/bin/sh",
            &["-c", &format!("cat <<'EOF'\n{WITH_PEERS_JSON}\nEOF")],
        )
        .expect("fetch_local misslyckades");
        assert_eq!(status.backend_state, "Running");
        assert_eq!(status.suggested_hosts()[0].1, "100.64.0.2");
    }

    #[test]
    fn fetch_local_returns_error_on_non_zero_exit() {
        let err = fetch_local(
            "/bin/sh",
            &["-c", "echo 'tailscale: not logged in' >&2; exit 1"],
        )
        .expect_err("förväntade att fetch_local skulle ge ett fel");
        match err {
            TailscaleError::LocalCommandFailed { exit_code, stderr } => {
                assert_eq!(exit_code, 1);
                assert!(stderr.contains("not logged in"));
            }
            other => panic!("fel feltyp: {other:?}"),
        }
    }
}
