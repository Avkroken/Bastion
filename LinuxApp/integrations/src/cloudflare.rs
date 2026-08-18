//! Cloudflare Tunnel via `cloudflared` över SSH. Sjätte integrationen.
//!
//! # Varför tunnlar och inte Cloudflares HTTP-API
//!
//! VISION räknar upp Cloudflare bland plugins, i sällskap med Docker,
//! Proxmox, TrueNAS och Unraid — alltså saker som KÖR på en server man
//! ansluter till. Det Cloudflare kör på en server är `cloudflared`.
//!
//! Att i stället prata med `api.cloudflare.com` hade brutit modellen på
//! tre sätt: paketet hade behövt en HTTP-klient och därmed tappat sin
//! beroendefrihet, en API-token hade behövt lagras och skyddas, och
//! resultatet hade inte handlat om värden man är ansluten till utan om
//! ett konto. Det är en annan produkt än en SSH-klient.
//!
//! Gränsen är alltså inte en begränsning utan ett val: appen visar vad
//! tunneln på DEN HÄR maskinen gör.
//!
//! # Anslutningar är det som betyder något
//!
//! En tunnel kan finnas, vara korrekt konfigurerad och ändå inte
//! förmedla någon trafik, för att `cloudflared` inte kör eller inte når
//! ut. Skillnaden syns bara på antalet aktiva anslutningar, och det är
//! därför den och inte tunnelns existens som avgör om raden ser frisk ut.

use serde_json::Value;

/// En tunnel som kontot känner till.
#[derive(Debug, Clone, PartialEq)]
pub struct Tunnel {
    pub id: String,
    pub name: String,
    /// En anslutning per edge-datacenter `cloudflared` nått.
    pub connections: Vec<Connection>,
}

impl Tunnel {
    /// Förmedlar tunneln trafik just nu?
    ///
    /// Noll anslutningar betyder att den finns men är nere. En tunnel som
    /// bara väntar på återanslutning räknas inte som uppe heller — den
    /// tar ingen trafik under tiden.
    pub fn is_up(&self) -> bool {
        self.connections.iter().any(|c| !c.pending_reconnect)
    }

    /// Datacentren tunneln är ansluten till, utan dubbletter.
    ///
    /// `cloudflared` öppnar normalt fyra anslutningar fördelade på två
    /// colos. Att lista `ARN, ARN, HEL, HEL` vore brus; två namn säger
    /// samma sak.
    pub fn colos(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for c in &self.connections {
            if !c.colo.is_empty() && !seen.contains(&c.colo) {
                seen.push(c.colo.clone());
            }
        }
        seen
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Connection {
    /// Cloudflares kod för datacentret, t.ex. `ARN` (Stockholm).
    pub colo: String,
    pub pending_reconnect: bool,
}

/// Tunnelnamn får innehålla bokstäver, siffror, bindestreck och
/// understreck. Ett id är en UUID, som matchar samma mönster.
///
/// Sjätte distinkta valideringsregeln i sex integrationer.
pub fn validate_tunnel(name: &str) -> Result<&str, String> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(name)
    } else {
        Err(format!("ogiltigt tunnelnamn: {name:?}"))
    }
}

pub fn tunnels_command() -> String {
    "cloudflared tunnel list --output json 2>/dev/null".to_string()
}

/// Detaljer för EN tunnel, inklusive vilka anslutningar som lever.
pub fn tunnel_info_command(name: &str) -> Result<String, String> {
    Ok(format!(
        "cloudflared tunnel info --output json {} 2>&1",
        validate_tunnel(name)?
    ))
}

/// Tjänstens tillstånd på värden.
///
/// `cloudflared` körs normalt som en systemd-tjänst. Att tunneln finns i
/// listan säger ingenting om huruvida daemonen kör — och det är just den
/// skillnaden som förklarar en tunnel utan anslutningar.
pub fn service_status_command() -> String {
    "systemctl is-active cloudflared 2>&1; cloudflared --version 2>/dev/null".to_string()
}

fn text(item: &Value, key: &str) -> String {
    item.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// `cloudflared tunnel list --output json` ger en array.
///
/// Fälten läses defensivt: Cloudflare har lagt till fält mellan
/// versioner, och en post utan `connections` (äldre utdata) ska bli en
/// tunnel som är NERE, inte en post som hoppas över.
pub fn parse_tunnels(output: &str) -> Vec<Tunnel> {
    let Ok(value) = serde_json::from_str::<Value>(output.trim()) else {
        return Vec::new();
    };
    let Some(array) = value.as_array() else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|item| {
            let name = text(item, "name");
            if name.is_empty() {
                return None;
            }
            let connections = item
                .get("connections")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .map(|c| Connection {
                            colo: text(c, "colo_name"),
                            pending_reconnect: c
                                .get("is_pending_reconnect")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(Tunnel { id: text(item, "id"), name, connections })
        })
        .collect()
}

/// Tjänstens tillstånd ur `systemctl is-active` plus versionsraden.
///
/// `systemctl is-active` svarar med ETT ord (`active`, `inactive`,
/// `failed`, `unknown`) och sätter exitkod därefter — men exitkoden går
/// förlorad när två kommandon kedjas, så ordet är det vi går på.
pub fn parse_service_status(output: &str) -> (String, Option<String>) {
    let mut lines = output.lines().map(str::trim).filter(|l| !l.is_empty());
    let state = lines.next().unwrap_or("okänt").to_string();
    let version = lines.find(|l| l.contains("cloudflared")).map(str::to_string);
    (state, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verklig form på `cloudflared tunnel list --output json`, förkortad.
    const TUNNELS: &str = r#"[
      {
        "id": "6ff42ae2-765d-4adf-8112-31c55c1551ef",
        "name": "homelab",
        "connections": [
          {"colo_name": "ARN", "is_pending_reconnect": false},
          {"colo_name": "ARN", "is_pending_reconnect": false},
          {"colo_name": "HEL", "is_pending_reconnect": false},
          {"colo_name": "HEL", "is_pending_reconnect": false}
        ]
      },
      {
        "id": "8a1b3c4d-0000-0000-0000-000000000000",
        "name": "gammal-tunnel",
        "connections": []
      }
    ]"#;

    /// Kärnan i vyn: en tunnel kan FINNAS och ändå vara nere. Det syns
    /// bara på anslutningarna, inte på att posten existerar.
    #[test]
    fn a_tunnel_without_connections_exists_but_is_down() {
        let tunnels = parse_tunnels(TUNNELS);
        assert_eq!(tunnels.len(), 2);
        assert!(tunnels[0].is_up());
        assert!(!tunnels[1].is_up(), "noll anslutningar betyder nere");
        assert_eq!(tunnels[1].name, "gammal-tunnel");
    }

    /// En tunnel som bara väntar på återanslutning tar ingen trafik och
    /// ska därför inte räknas som uppe.
    #[test]
    fn pending_reconnect_does_not_count_as_up() {
        let out = r#"[{"name": "t", "connections": [
            {"colo_name": "ARN", "is_pending_reconnect": true}
        ]}]"#;
        let tunnels = parse_tunnels(out);
        assert_eq!(tunnels.len(), 1);
        assert!(!tunnels[0].is_up());

        // Men EN levande anslutning räcker, även om andra väntar.
        let mixed = r#"[{"name": "t", "connections": [
            {"colo_name": "ARN", "is_pending_reconnect": true},
            {"colo_name": "HEL", "is_pending_reconnect": false}
        ]}]"#;
        assert!(parse_tunnels(mixed)[0].is_up());
    }

    /// cloudflared öppnar normalt fyra anslutningar över två colos. Att
    /// lista ARN, ARN, HEL, HEL vore brus.
    #[test]
    fn duplicate_colos_are_collapsed() {
        let tunnels = parse_tunnels(TUNNELS);
        assert_eq!(tunnels[0].colos(), vec!["ARN", "HEL"]);
        assert!(tunnels[1].colos().is_empty());
    }

    /// Äldre utdata saknar `connections`. Posten ska bli en tunnel som är
    /// NERE, inte hoppas över — annars försvinner den ur listan helt.
    #[test]
    fn missing_connections_field_yields_a_down_tunnel_not_a_dropped_row() {
        let tunnels = parse_tunnels(r#"[{"id": "x", "name": "gammal"}]"#);
        assert_eq!(tunnels.len(), 1);
        assert_eq!(tunnels[0].name, "gammal");
        assert!(!tunnels[0].is_up());
    }

    #[test]
    fn sixth_validation_rule_rejects_injection() {
        for good in ["homelab", "min-tunnel", "tunnel_1", "6ff42ae2-765d-4adf-8112-31c55c1551ef"] {
            assert!(validate_tunnel(good).is_ok(), "{good}");
        }
        for bad in ["", "min tunnel", "t; rm -rf /", "t$(id)", "t`id`", "t'", &"a".repeat(65)] {
            assert!(validate_tunnel(bad).is_err(), "{bad:?}");
            assert!(tunnel_info_command(bad).is_err());
        }
        assert_eq!(
            tunnel_info_command("homelab").unwrap(),
            "cloudflared tunnel info --output json homelab 2>&1"
        );
    }

    #[test]
    fn non_json_output_yields_no_tunnels() {
        for bad in ["", "   ", "cloudflared: command not found", "{\"inte\": \"array\"}", "null"] {
            assert!(parse_tunnels(bad).is_empty(), "{bad:?}");
        }
        // Poster utan namn hoppas över — en rad utan namn går inte att
        // agera på.
        assert!(parse_tunnels(r#"[{"id": "x"}]"#).is_empty());
    }

    /// Att tunneln finns i listan säger ingenting om huruvida daemonen
    /// kör — och det är just den skillnaden som förklarar en tunnel utan
    /// anslutningar.
    #[test]
    fn service_state_and_version_are_read_from_the_combined_output() {
        let (state, version) = parse_service_status("active\ncloudflared version 2026.8.0 (built …)");
        assert_eq!(state, "active");
        assert!(version.unwrap().contains("2026.8.0"));

        let (state, version) = parse_service_status("inactive\n");
        assert_eq!(state, "inactive");
        assert_eq!(version, None, "utan version ska fältet vara tomt, inte gissat");

        let (state, _) = parse_service_status("");
        assert_eq!(state, "okänt");
    }
}
