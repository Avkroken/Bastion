//! Unraid via `mdcmd` över SSH. Femte integrationen i paketet.
//!
//! # Tredje utdataformatet i paketet
//!
//! Docker, Kubernetes och Proxmox svarar med kolumner. TrueNAS svarar med
//! JSON. `mdcmd status` svarar med `nyckel=värde`, en per rad, där
//! diskarna är INDEXERADE i nyckeln:
//!
//! ```text
//! mdState=STARTED
//! mdResync=0
//! diskName.0=md1
//! diskSize.0=3907018532
//! diskState.0=7
//! ```
//!
//! Att det är en tredje form är i sig ett argument för att
//! `refresh_integration_list` inte försöker äga parsningen: skelettet tar
//! en sträng och en stängning, och vad som händer däremellan är
//! modulens ensak.
//!
//! # Varför diskarnas tillståndskoder visas råa
//!
//! `diskState.N` är ett heltal, och betydelsen är inte stabilt
//! dokumenterad av Unraid — den har ändrats mellan versioner. Att gissa
//! en översättningstabell hade gett en rad som SER auktoritativ ut men
//! kan vara fel, vilket är sämre än en siffra användaren kan slå upp.
//! `mdState` däremot är en sträng (`STARTED`/`STOPPED`) och tolkas.

use std::collections::HashMap;

/// Arrayens övergripande tillstånd.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayStatus {
    /// `STARTED`, `STOPPED` eller vad `mdcmd` nu svarar.
    pub state: String,
    pub disk_count: Option<u32>,
    pub disabled_count: Option<u32>,
    /// Pågår en paritetskontroll eller ombyggnad?
    pub resync: Option<ResyncProgress>,
}

impl ArrayStatus {
    pub fn is_started(&self) -> bool {
        self.state == "STARTED"
    }

    /// En avstängd disk betyder att arrayen kör på paritet. Data finns
    /// kvar, men nästa diskfel är ett datafel — det är skillnaden mellan
    /// "allt är bra" och "åtgärda nu".
    pub fn has_disabled_disks(&self) -> bool {
        self.disabled_count.unwrap_or(0) > 0
    }
}

/// En pågående paritetskontroll eller ombyggnad.
#[derive(Debug, Clone, PartialEq)]
pub struct ResyncProgress {
    pub position: u64,
    pub total: u64,
}

impl ResyncProgress {
    /// Andel klart, 0.0–1.0.
    ///
    /// `total` noll betyder att ingen resync pågår, och då finns ingen
    /// andel att räkna — inte "noll procent klart", som hade sett ut som
    /// en resync som står stilla.
    pub fn fraction(&self) -> Option<f64> {
        if self.total == 0 {
            return None;
        }
        Some((self.position as f64 / self.total as f64).clamp(0.0, 1.0))
    }
}

/// En disk i arrayen.
#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
    pub slot: u32,
    pub name: String,
    /// Storleken som `mdcmd` rapporterar den: i 1024-byteblock.
    pub size_blocks: u64,
    /// Rå tillståndskod. Se modulkommentaren om varför den inte tolkas.
    pub state: String,
}

impl Disk {
    pub fn size_bytes(&self) -> u64 {
        self.size_blocks.saturating_mul(1024)
    }
}

pub fn status_command() -> String {
    "mdcmd status 2>/dev/null".to_string()
}

/// Delade mappar är kataloger under `/mnt/user`.
///
/// Unraid har ingen CLI som listar dem — webbgränssnittet läser
/// `/boot/config/shares/*.cfg`. Katalogerna är enklare och säger samma
/// sak om vad som FINNS; konfigurationen säger hur de är inställda, och
/// det är en annan fråga än den här vyn ställer.
pub fn shares_command() -> String {
    "ls -1 /mnt/user 2>/dev/null".to_string()
}

/// `nyckel=värde` per rad till en uppslagstabell.
///
/// Delar på FÖRSTA likhetstecknet: ett värde kan innehålla fler, och
/// `split('=')` hade tappat allt efter det andra.
fn pairs(output: &str) -> HashMap<&str, &str> {
    output
        .lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once('=')?;
            if key.is_empty() { None } else { Some((key, value)) }
        })
        .collect()
}

pub fn parse_status(output: &str) -> Option<ArrayStatus> {
    let map = pairs(output);
    // Utan `mdState` är det inte ett mdcmd-svar. Att bygga en status av
    // tomma fält hade gett en vy som ser fungerande ut mot en maskin som
    // inte är en Unraid alls.
    let state = (*map.get("mdState")?).to_string();

    let number = |key: &str| map.get(key).and_then(|v| v.parse::<u64>().ok());
    let resync_total = number("mdResync").unwrap_or(0);
    Some(ArrayStatus {
        state,
        disk_count: number("mdNumDisks").map(|n| n as u32),
        disabled_count: number("mdNumDisabled").map(|n| n as u32),
        resync: if resync_total > 0 {
            Some(ResyncProgress {
                position: number("mdResyncPos").unwrap_or(0),
                total: resync_total,
            })
        } else {
            None
        },
    })
}

/// Plockar ut diskarna ur de indexerade nycklarna.
///
/// En disk räknas som närvarande först när den har ett NAMN. Unraid
/// rapporterar tomma slottar med `diskName.N=` (tomt värde), och de ska
/// inte bli rader.
pub fn parse_disks(output: &str) -> Vec<Disk> {
    let map = pairs(output);
    let mut disks: Vec<Disk> = map
        .keys()
        .filter_map(|key| key.strip_prefix("diskName."))
        .filter_map(|index| {
            let slot: u32 = index.parse().ok()?;
            let name = map.get(format!("diskName.{slot}").as_str())?.trim();
            if name.is_empty() {
                return None;
            }
            Some(Disk {
                slot,
                name: name.to_string(),
                size_blocks: map
                    .get(format!("diskSize.{slot}").as_str())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                state: map
                    .get(format!("diskState.{slot}").as_str())
                    .unwrap_or(&"")
                    .to_string(),
            })
        })
        .collect();
    // Slotordning, inte hashordning — annars hoppar listan mellan
    // uppdateringar.
    disks.sort_by_key(|d| d.slot);
    disks
}

pub fn parse_shares(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verklig `mdcmd status`-utdata, förkortad men med formatet intakt:
    /// nyckel=värde, diskarna indexerade i nyckeln.
    const STATUS: &str = "\
sbName=/boot/config/super.dat
sbVersion=2.9.13
mdState=STARTED
mdNumDisks=3
mdNumDisabled=0
mdResync=0
mdResyncPos=0
diskNumber.0=0
diskName.0=md1
diskSize.0=3907018532
diskState.0=7
diskNumber.1=1
diskName.1=md2
diskSize.1=1953514552
diskState.1=7
diskNumber.2=2
diskName.2=
diskSize.2=0
diskState.2=0";

    #[test]
    fn array_status_is_read_from_key_value_pairs() {
        let status = parse_status(STATUS).expect("mdState finns");
        assert!(status.is_started());
        assert_eq!(status.disk_count, Some(3));
        assert!(!status.has_disabled_disks());
        assert!(status.resync.is_none(), "mdResync=0 betyder ingen pågående kontroll");
    }

    /// Utan `mdState` är svaret inte från mdcmd. Att bygga en status av
    /// tomma fält hade gett en vy som ser fungerande ut mot en maskin som
    /// inte är en Unraid alls.
    #[test]
    fn a_response_without_md_state_is_not_a_status() {
        assert!(parse_status("").is_none());
        assert!(parse_status("bash: mdcmd: command not found").is_none());
        assert!(parse_status("sbVersion=2.9.13\nmdNumDisks=3").is_none());
    }

    /// Tomma slottar rapporteras med `diskName.N=` och ska inte bli rader.
    #[test]
    fn empty_slots_are_not_disks_and_order_follows_the_slot() {
        let disks = parse_disks(STATUS);
        assert_eq!(disks.len(), 2, "den tomma slotten ska inte bli en disk");
        assert_eq!(disks[0].slot, 0);
        assert_eq!(disks[0].name, "md1");
        assert_eq!(disks[1].name, "md2");
        // Sortering på slot, inte hashordning — annars hoppar listan
        // mellan uppdateringar.
        assert!(disks[0].slot < disks[1].slot);
    }

    #[test]
    fn disk_size_is_reported_in_1024_byte_blocks() {
        let disks = parse_disks(STATUS);
        assert_eq!(disks[0].size_blocks, 3_907_018_532);
        assert_eq!(disks[0].size_bytes(), 3_907_018_532 * 1024);
    }

    /// En avstängd disk betyder att arrayen kör på paritet: data finns
    /// kvar, men nästa diskfel är ett datafel.
    #[test]
    fn a_disabled_disk_is_reported_even_when_the_array_is_started() {
        let out = "mdState=STARTED\nmdNumDisks=3\nmdNumDisabled=1\nmdResync=0";
        let status = parse_status(out).unwrap();
        assert!(status.is_started(), "arrayen kör fortfarande");
        assert!(status.has_disabled_disks(), "men på paritet");
    }

    /// Noll som total betyder INGEN resync, inte "noll procent klart" —
    /// det senare hade sett ut som en kontroll som står stilla.
    #[test]
    fn resync_progress_is_absent_rather_than_zero_when_nothing_runs() {
        let running = parse_status("mdState=STARTED\nmdResync=1000\nmdResyncPos=250")
            .unwrap()
            .resync
            .expect("en pågående resync");
        assert_eq!(running.fraction(), Some(0.25));

        assert!(parse_status("mdState=STARTED\nmdResync=0\nmdResyncPos=0").unwrap().resync.is_none());

        // Och en position bortom totalen ska inte ge över 100 procent.
        let odd = ResyncProgress { position: 2000, total: 1000 };
        assert_eq!(odd.fraction(), Some(1.0));
        assert_eq!(ResyncProgress { position: 0, total: 0 }.fraction(), None);
    }

    /// Ett värde kan innehålla likhetstecken — sökvägar och base64 gör
    /// det. Delningen sker på det FÖRSTA.
    #[test]
    fn values_may_contain_equals_signs() {
        let status = parse_status("mdState=STARTED\nsbName=/boot/config/super.dat?a=b=c").unwrap();
        assert!(status.is_started());
        let disks = parse_disks("diskName.0=md=1\ndiskSize.0=100\ndiskState.0=7");
        assert_eq!(disks[0].name, "md=1");
    }

    #[test]
    fn shares_are_directory_names_and_blank_lines_are_skipped() {
        assert_eq!(parse_shares("appdata\nisos\n\n  domains  \n"), vec!["appdata", "isos", "domains"]);
        assert!(parse_shares("").is_empty());
        assert!(parse_shares("\n\n").is_empty());
    }
}
