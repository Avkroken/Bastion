//! TrueNAS via `midclt` över SSH. Fjärde integrationen i paketet.
//!
//! # Varför `midclt` och inte `zpool`/`systemctl`
//!
//! TrueNAS är ett APPLIANCE. Konfigurationen ägs av middleware-daemonen,
//! och det som ändras utanför den skrivs över vid nästa omkonfiguration
//! eller uppgradering. `midclt call` är samma API som webbgränssnittet
//! använder, vilket betyder att det appen visar är det maskinen faktiskt
//! tycker — och att en tjänst som startas här stannar startad.
//!
//! Att läsa `zpool status` hade gett nästan samma poolinformation och
//! varit frestande enkelt, men `service.start` har ingen motsvarighet i
//! `systemctl` på en TrueNAS: middleware skulle inte veta om det.
//!
//! # JSON, till skillnad från de andra tre
//!
//! Docker, Kubernetes och Proxmox svarar med kolumner. `midclt` svarar
//! med JSON, och det är bättre här: fälten är namngivna, så en extra
//! kolumn i en framtida version bryter ingenting. Det är också varför
//! paketet över huvud taget har `serde_json` som beroende.

use serde_json::Value;

/// En ZFS-pool.
#[derive(Debug, Clone, PartialEq)]
pub struct Pool {
    pub name: String,
    /// `ONLINE`, `DEGRADED`, `FAULTED`, `OFFLINE`, `UNAVAIL`, `REMOVED`.
    pub status: String,
    pub healthy: bool,
}

impl Pool {
    /// `healthy` kommer från middleware och är INTE samma sak som
    /// `status == "ONLINE"`.
    ///
    /// En pool kan vara ONLINE och ändå ohälsosam — pågående resilver,
    /// läsfel som inte tagit ner en disk, eller en scrub som hittat
    /// checksummefel. Att härleda hälsan ur statussträngen hade dolt
    /// precis de fallen, alltså de enda som är värda en varning.
    pub fn needs_attention(&self) -> bool {
        !self.healthy || self.status != "ONLINE"
    }
}

/// En tjänst i TrueNAS (SMB, NFS, SSH …).
#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    /// Middlewares egen identifierare, t.ex. `cifs`, `nfs`, `ssh`.
    pub id: String,
    /// `RUNNING` eller `STOPPED`.
    pub state: String,
    /// Startar tjänsten vid uppstart?
    pub enabled: bool,
}

impl Service {
    pub fn is_running(&self) -> bool {
        self.state == "RUNNING"
    }

    /// Kör men startar inte vid uppstart — överlever alltså inte en
    /// omstart. Värt att visa: det är nästan alltid oavsiktligt.
    pub fn is_running_but_not_enabled(&self) -> bool {
        self.is_running() && !self.enabled
    }
}

/// En larmnotis från middleware.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub level: String,
    pub formatted: String,
    pub dismissed: bool,
}

impl Alert {
    /// `CRITICAL` och `ERROR` är de nivåer som betyder att något är
    /// trasigt nu. `WARNING` och nedåt är information.
    pub fn is_critical(&self) -> bool {
        matches!(self.level.as_str(), "CRITICAL" | "ERROR" | "ALERT" | "EMERGENCY")
    }
}

/// Tjänste-id:n i TrueNAS är korta gemena ord (`cifs`, `nfs`, `ssh`,
/// `smartd`). Regeln är avsiktligt snäv: den ska släppa igenom exakt de
/// namnen och ingenting annat, vilket gör den till ett fullgott
/// injektionsskydd på köpet.
///
/// Fjärde distinkta valideringsregeln i fyra integrationer, vilket är
/// precis varför varje modul äger sin egen.
pub fn validate_service(id: &str) -> Result<&str, String> {
    let ok = !id.is_empty()
        && id.len() <= 32
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(id)
    } else {
        Err(format!("ogiltigt tjänste-id: {id:?}"))
    }
}

pub fn pools_command() -> String {
    "midclt call pool.query 2>/dev/null".to_string()
}

pub fn services_command() -> String {
    "midclt call service.query 2>/dev/null".to_string()
}

/// Bara det som inte redan avfärdats. Ett avfärdat larm är per
/// definition sett och hanterat, och att visa det igen gör listan
/// obrukbar på en maskin som stått ett tag.
pub fn alerts_command() -> String {
    "midclt call alert.list 2>/dev/null".to_string()
}

/// `midclt call service.start '\"cifs\"'` — argumentet är JSON, alltså en
/// CITERAD sträng, och citattecknen måste överleva skalet.
///
/// Enkla citattecken runt hela JSON-argumentet är säkert eftersom
/// [`validate_service`] redan uteslutit `'` (och allt annat som inte är
/// gemener, siffror eller understreck).
fn service_command(verb: &str, id: &str) -> Result<String, String> {
    Ok(format!("midclt call service.{verb} '\"{}\"' 2>&1", validate_service(id)?))
}

pub fn start_service_command(id: &str) -> Result<String, String> {
    service_command("start", id)
}

pub fn stop_service_command(id: &str) -> Result<String, String> {
    service_command("stop", id)
}

pub fn restart_service_command(id: &str) -> Result<String, String> {
    service_command("restart", id)
}

/// Tolkar `midclt`-svar till en JSON-array.
///
/// Svaret är en array vid framgång. Vid fel skriver `midclt` ett
/// pythonliknande traceback på stderr — som gått till `/dev/null` — och
/// ingenting på stdout, så tomt in ska ge tomt ut i stället för ett
/// halvtolkat resultat.
fn array(output: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(output.trim())
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

fn text(item: &Value, key: &str) -> String {
    item.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

pub fn parse_pools(output: &str) -> Vec<Pool> {
    array(output)
        .iter()
        .filter_map(|item| {
            let name = text(item, "name");
            if name.is_empty() {
                return None;
            }
            Some(Pool {
                name,
                status: text(item, "status"),
                // Saknat `healthy` tolkas som OHÄLSOSAMT. En äldre
                // middleware utan fältet ska ge en varning att titta
                // närmare, inte ett tyst godkännande.
                healthy: item.get("healthy").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

pub fn parse_services(output: &str) -> Vec<Service> {
    array(output)
        .iter()
        .filter_map(|item| {
            let id = text(item, "service");
            if id.is_empty() {
                return None;
            }
            Some(Service {
                id,
                state: text(item, "state"),
                enabled: item.get("enable").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

pub fn parse_alerts(output: &str) -> Vec<Alert> {
    array(output)
        .iter()
        .filter_map(|item| {
            let formatted = text(item, "formatted");
            if formatted.is_empty() {
                return None;
            }
            Some(Alert {
                level: text(item, "level"),
                formatted,
                dismissed: item.get("dismissed").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fjärde distinkta valideringsregeln i fyra integrationer. Snäv med
    /// flit: den ska släppa igenom middlewares egna tjänste-id:n och
    /// ingenting annat.
    #[test]
    fn only_middleware_service_ids_are_valid() {
        for good in ["cifs", "nfs", "ssh", "smartd", "iscsitarget", "s3"] {
            assert!(validate_service(good).is_ok(), "{good} skulle accepterats");
        }
        for bad in ["CIFS", "nfs-server", "nfs.service", "", &"a".repeat(33)] {
            assert!(validate_service(bad).is_err(), "{bad:?} skulle avvisats");
        }
    }

    /// Argumentet till midclt är JSON, alltså en CITERAD sträng inuti
    /// skalcitationen. Att valideringen uteslutit `'` är det som gör den
    /// konstruktionen säker.
    #[test]
    fn service_commands_wrap_the_id_as_json_inside_shell_quotes() {
        assert_eq!(
            start_service_command("cifs").unwrap(),
            "midclt call service.start '\"cifs\"' 2>&1"
        );
        assert_eq!(
            restart_service_command("nfs").unwrap(),
            "midclt call service.restart '\"nfs\"' 2>&1"
        );
        for bad in ["cifs'; rm -rf /", "cifs\"", "cifs$(id)", "cifs`id`", "a b"] {
            assert!(validate_service(bad).is_err(), "{bad:?}");
            assert!(start_service_command(bad).is_err());
            assert!(stop_service_command(bad).is_err());
            assert!(restart_service_command(bad).is_err());
        }
    }

    /// Kärnan i hela poolvyn: `healthy` är INTE samma sak som
    /// `status == "ONLINE"`. En pool under resilver är ONLINE men
    /// ohälsosam, och det är precis det fallet som är värt en varning.
    #[test]
    fn an_online_pool_can_still_be_unhealthy() {
        let out = r#"[
            {"name": "tank", "status": "ONLINE", "healthy": true},
            {"name": "backup", "status": "ONLINE", "healthy": false},
            {"name": "gammal", "status": "DEGRADED", "healthy": false}
        ]"#;
        let pools = parse_pools(out);
        assert_eq!(pools.len(), 3);
        assert!(!pools[0].needs_attention());
        assert!(pools[1].needs_attention(), "ONLINE men ohälsosam ska varna");
        assert!(pools[2].needs_attention());
    }

    /// En äldre middleware utan `healthy` ska ge en varning att titta
    /// närmare, inte ett tyst godkännande.
    #[test]
    fn a_missing_healthy_field_counts_as_unhealthy() {
        let pools = parse_pools(r#"[{"name": "tank", "status": "ONLINE"}]"#);
        assert_eq!(pools.len(), 1);
        assert!(!pools[0].healthy);
        assert!(pools[0].needs_attention());
    }

    /// Kör men startar inte vid uppstart — överlever alltså inte en
    /// omstart. Nästan alltid oavsiktligt, och därför värt en egen fråga.
    #[test]
    fn a_running_service_that_is_not_enabled_is_flagged() {
        let out = r#"[
            {"service": "cifs", "state": "RUNNING", "enable": true},
            {"service": "nfs",  "state": "RUNNING", "enable": false},
            {"service": "ssh",  "state": "STOPPED", "enable": true}
        ]"#;
        let services = parse_services(out);
        assert_eq!(services.len(), 3);
        assert!(!services[0].is_running_but_not_enabled());
        assert!(services[1].is_running_but_not_enabled(), "startar inte efter omstart");
        assert!(!services[2].is_running_but_not_enabled(), "stoppad kan inte tappa något");
        assert!(!services[2].is_running());
    }

    #[test]
    fn alert_levels_separate_broken_from_informational() {
        let out = r#"[
            {"level": "CRITICAL", "formatted": "Pool tank is DEGRADED", "dismissed": false},
            {"level": "WARNING",  "formatted": "Ny uppdatering finns", "dismissed": false},
            {"level": "ERROR",    "formatted": "Disk sda har fel", "dismissed": true}
        ]"#;
        let alerts = parse_alerts(out);
        assert_eq!(alerts.len(), 3);
        assert!(alerts[0].is_critical());
        assert!(!alerts[1].is_critical(), "WARNING är information, inte trasigt");
        assert!(alerts[2].is_critical());
        assert!(alerts[2].dismissed, "avfärdat ska gå att skilja ut");
    }

    /// `midclt` skriver sitt traceback på stderr, som gått till
    /// /dev/null. Tomt in ska ge tomt ut — inte ett halvtolkat resultat.
    #[test]
    fn non_json_and_empty_output_yield_nothing() {
        for bad in ["", "   ", "Traceback (most recent call last):", "{\"inte\": \"en array\"}", "null"] {
            assert!(parse_pools(bad).is_empty(), "{bad:?}");
            assert!(parse_services(bad).is_empty(), "{bad:?}");
            assert!(parse_alerts(bad).is_empty(), "{bad:?}");
        }
        // Poster utan sitt nyckelfält hoppas över i stället för att bli
        // rader utan namn.
        assert!(parse_pools(r#"[{"status": "ONLINE"}]"#).is_empty());
        assert!(parse_services(r#"[{"state": "RUNNING"}]"#).is_empty());
        assert!(parse_alerts(r#"[{"level": "CRITICAL"}]"#).is_empty());
    }
}
