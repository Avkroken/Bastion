//! Proxmox VE-integration via `qm`, `pct` och `pvesm` över SSH. Tredje
//! integrationen bredvid [`crate::docker`] och [`crate::kubernetes`], och
//! avsiktligt byggd genom samma skelett (`refresh_integration_list`) —
//! den är lika mycket ett prov på om abstraktionen håller som en
//! funktion i sig.
//!
//! # Varför tre olika verktyg och inte `pvesh`
//!
//! `pvesh get /nodes/…` ger JSON och vore enhetligt, men kräver att man
//! vet nodnamnet i sökvägen — och det man vill visa är just vad som finns
//! på noden man loggat in på. `qm`/`pct`/`pvesm` verkar lokalt utan att
//! behöva veta det, och är dessutom vad en Proxmox-administratör redan
//! har i fingrarna.
//!
//! # Identifierare är heltal, inte namn
//!
//! Den tredje distinkta valideringsregeln i tre integrationer, vilket är
//! själva poängen med att låta varje modul äga sin: Docker tillåter
//! versaler och punkter, Kubernetes bara RFC 1123-etiketter, och Proxmox
//! adresserar allt med ett VMID — ett heltal mellan 100 och 999999999.
//! En regel som bara accepterar siffror är samtidigt det starkaste
//! injektionsskyddet av de tre.

/// Ett VMID, validerat.
///
/// Proxmox reserverar 1–99 internt; användarskapade gäster börjar på 100.
/// Övre gränsen är den `pvesh` självt avvisar över.
pub fn validate_vmid(vmid: &str) -> Result<&str, String> {
    let ok = !vmid.is_empty()
        && vmid.len() <= 9
        && vmid.chars().all(|c| c.is_ascii_digit())
        && vmid.parse::<u64>().map(|n| n >= 100).unwrap_or(false);
    if ok {
        Ok(vmid)
    } else {
        Err(format!("ogiltigt VMID: {vmid:?}"))
    }
}

/// En virtuell maskin (`qm`) eller en LXC-container (`pct`).
///
/// Samma typ för båda: de skiljer sig i vilket verktyg som styr dem, inte
/// i vad användaren vill se. Vilket verktyg det är bärs av [`Guest::kind`].
#[derive(Debug, Clone, PartialEq)]
pub struct Guest {
    pub vmid: String,
    pub name: String,
    pub status: String,
    pub kind: GuestKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GuestKind {
    /// KVM-maskin, styrs med `qm`.
    Vm,
    /// LXC-container, styrs med `pct`.
    Container,
}

impl GuestKind {
    fn tool(self) -> &'static str {
        match self {
            GuestKind::Vm => "qm",
            GuestKind::Container => "pct",
        }
    }
}

impl Guest {
    pub fn is_running(&self) -> bool {
        self.status == "running"
    }
}

/// Lagring på noden.
#[derive(Debug, Clone, PartialEq)]
pub struct Storage {
    pub name: String,
    pub kind: String,
    pub status: String,
    /// Använt i procent, som text (`73.4%`). Lämnas oparsat: siffran är
    /// till för att läsas, och ett fel i tolkningen vore värre än att
    /// visa Proxmox egen formatering.
    pub used_percent: String,
}

impl Storage {
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }
}

pub fn vms_command() -> String {
    "qm list 2>/dev/null".to_string()
}

pub fn containers_command() -> String {
    "pct list 2>/dev/null".to_string()
}

pub fn storage_command() -> String {
    "pvesm status 2>/dev/null".to_string()
}

/// Startar en gäst. `qm start` respektive `pct start`.
pub fn start_command(kind: GuestKind, vmid: &str) -> Result<String, String> {
    Ok(format!("{} start {} 2>&1", kind.tool(), validate_vmid(vmid)?))
}

/// Ren avstängning via gästens eget OS.
///
/// `shutdown` och inte `stop`: `stop` motsvarar att dra ur strömmen och
/// riskerar filsystemsskador. Den som verkligen vill det har en egen
/// knapp — se [`stop_command`].
pub fn shutdown_command(kind: GuestKind, vmid: &str) -> Result<String, String> {
    Ok(format!("{} shutdown {} 2>&1", kind.tool(), validate_vmid(vmid)?))
}

/// Hård avstängning. Motsvarar att dra ur strömmen.
pub fn stop_command(kind: GuestKind, vmid: &str) -> Result<String, String> {
    Ok(format!("{} stop {} 2>&1", kind.tool(), validate_vmid(vmid)?))
}

/// Aktuell status och konfiguration för en gäst.
pub fn status_command(kind: GuestKind, vmid: &str) -> Result<String, String> {
    Ok(format!("{} config {} 2>&1", kind.tool(), validate_vmid(vmid)?))
}

/// Delar en rad på mellanslag och kräver minst `expected` fält.
fn fields(line: &str, expected: usize) -> Option<Vec<&str>> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < expected {
        return None;
    }
    Some(parts)
}

/// Är raden rubrikraden? Alla tre verktygen skriver ut en, och
/// `--no-headers` finns inte som flagga.
fn is_header(first_field: &str) -> bool {
    matches!(first_field, "VMID" | "Name")
}

/// `qm list` ger VMID NAME STATUS MEM(MB) BOOTDISK(GB) PID — fasta
/// kolumner, namnet på plats två.
pub fn parse_vms(output: &str) -> Vec<Guest> {
    output
        .lines()
        .filter_map(|line| {
            let f = fields(line, 3)?;
            if is_header(f[0]) {
                return None;
            }
            validate_vmid(f[0]).ok()?;
            Some(Guest {
                vmid: f[0].to_string(),
                name: f[1].to_string(),
                status: f[2].to_string(),
                kind: GuestKind::Vm,
            })
        })
        .collect()
}

/// `pct list` ger VMID Status Lock Name — och `Lock` är TOMT för en
/// gäst som inte är låst.
///
/// Det gör fältantalet varierande: en olåst container ger tre fält, en
/// låst fyra. Namnet är därför hämtat BAKIFRÅN, inte från ett fast
/// index. Ett Proxmox-gästnamn är ett värdnamn och kan inte innehålla
/// mellanslag, så sista fältet är alltid hela namnet.
pub fn parse_containers(output: &str) -> Vec<Guest> {
    output
        .lines()
        .filter_map(|line| {
            let f = fields(line, 3)?;
            if is_header(f[0]) {
                return None;
            }
            validate_vmid(f[0]).ok()?;
            Some(Guest {
                vmid: f[0].to_string(),
                status: f[1].to_string(),
                name: f[f.len() - 1].to_string(),
                kind: GuestKind::Container,
            })
        })
        .collect()
}

/// `pvesm status` ger Name Type Status Total Used Available %.
pub fn parse_storage(output: &str) -> Vec<Storage> {
    output
        .lines()
        .filter_map(|line| {
            let f = fields(line, 7)?;
            if is_header(f[0]) {
                return None;
            }
            Some(Storage {
                name: f[0].to_string(),
                kind: f[1].to_string(),
                status: f[2].to_string(),
                // Procenten är sista kolumnen.
                used_percent: f[f.len() - 1].to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tredje distinkta valideringsregeln i tre integrationer, och den
    /// snävaste: bara siffror. Att den är så snäv gör den samtidigt till
    /// det starkaste injektionsskyddet av de tre.
    #[test]
    fn only_integers_from_100_are_valid_vmids() {
        for good in ["100", "101", "9999", "123456789"] {
            assert!(validate_vmid(good).is_ok(), "{good} skulle accepterats");
        }
        for bad in ["", "99", "1", "0", "-100", "100 ", "10a", "1234567890", "web"] {
            assert!(validate_vmid(bad).is_err(), "{bad:?} skulle avvisats");
        }
    }

    #[test]
    fn injection_cannot_reach_any_command_builder() {
        for bad in ["100; rm -rf /", "100 && curl evil", "100$(id)", "100|tee x", "'100'"] {
            assert!(validate_vmid(bad).is_err(), "{bad:?}");
            for kind in [GuestKind::Vm, GuestKind::Container] {
                assert!(start_command(kind, bad).is_err());
                assert!(shutdown_command(kind, bad).is_err());
                assert!(stop_command(kind, bad).is_err());
                assert!(status_command(kind, bad).is_err());
            }
        }
    }

    /// Samma åtgärd, olika verktyg. Det är hela skillnaden mellan en VM
    /// och en container här.
    #[test]
    fn vms_use_qm_and_containers_use_pct() {
        assert_eq!(start_command(GuestKind::Vm, "100").unwrap(), "qm start 100 2>&1");
        assert_eq!(start_command(GuestKind::Container, "100").unwrap(), "pct start 100 2>&1");
        assert_eq!(shutdown_command(GuestKind::Vm, "101").unwrap(), "qm shutdown 101 2>&1");
        assert_eq!(stop_command(GuestKind::Container, "102").unwrap(), "pct stop 102 2>&1");
    }

    /// `shutdown` går via gästens OS, `stop` drar ur strömmen. Att de är
    /// olika kommandon och inte samma knapp är avsiktligt.
    #[test]
    fn shutdown_and_stop_are_different_commands() {
        let clean = shutdown_command(GuestKind::Vm, "100").unwrap();
        let hard = stop_command(GuestKind::Vm, "100").unwrap();
        assert_ne!(clean, hard);
        assert!(clean.contains("shutdown"));
        assert!(hard.contains(" stop "));
    }

    #[test]
    fn vm_list_is_parsed_with_the_header_skipped() {
        let out = "\
      VMID NAME                 STATUS     MEM(MB)    BOOTDISK(GB) PID
       100 web                  running    2048              32.00 1234
       101 db                   stopped    4096              64.00 0";
        let vms = parse_vms(out);
        assert_eq!(vms.len(), 2, "rubrikraden ska inte bli en gäst");
        assert_eq!(vms[0].vmid, "100");
        assert_eq!(vms[0].name, "web");
        assert!(vms[0].is_running());
        assert!(!vms[1].is_running());
        assert!(vms.iter().all(|v| v.kind == GuestKind::Vm));
    }

    /// Fällan i `pct list`: kolumnen `Lock` är TOM för en olåst
    /// container, så fältantalet varierar mellan tre och fyra. Namnet
    /// måste tas bakifrån — ett fast index ger fel för den ena eller
    /// andra.
    #[test]
    fn container_name_is_taken_from_the_end_because_lock_may_be_empty() {
        let out = "\
VMID       Status     Lock         Name
100        running                 pihole
101        stopped    backup       nextcloud";
        let cts = parse_containers(out);
        assert_eq!(cts.len(), 2);

        assert_eq!(cts[0].vmid, "100");
        assert_eq!(cts[0].name, "pihole", "olåst: tre fält, namnet sist");
        assert!(cts[0].is_running());

        assert_eq!(cts[1].name, "nextcloud", "låst: fyra fält, namnet ändå sist");
        assert_eq!(cts[1].status, "stopped");
        assert!(cts.iter().all(|c| c.kind == GuestKind::Container));
    }

    #[test]
    fn storage_reports_type_status_and_usage() {
        let out = "\
Name             Type     Status           Total            Used       Available        %
local             dir     active        98559220        12345678        81181542   12.53%
tank              zfs     active      3844505600      2818572288      1025933312   73.31%
backup            nfs   inactive               0               0               0    0.00%";
        let s = parse_storage(out);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].name, "local");
        assert_eq!(s[0].kind, "dir");
        assert!(s[0].is_active());
        assert_eq!(s[1].used_percent, "73.31%");
        assert!(!s[2].is_active(), "inactive ska inte räknas som aktiv");
    }

    /// Rader som inte börjar med ett giltigt VMID hoppas över — det
    /// fångar både rubriken och eventuella varningsrader som slunkit
    /// förbi `2>/dev/null`.
    #[test]
    fn junk_lines_are_skipped_rather_than_becoming_guests() {
        assert!(parse_vms("").is_empty());
        assert!(parse_vms("\n\n").is_empty());
        assert!(parse_containers("VMID Status Lock Name").is_empty(), "bara rubrik");
        assert!(parse_vms("could not connect to server").is_empty());
        assert!(parse_vms("99 reserverad running 1 1 1").is_empty(), "VMID under 100 är internt");
        assert!(parse_storage("Name Type Status").is_empty(), "för få fält");

        let vms = parse_vms("\n       100 web running 2048 32.00 1234\nskräprad\n");
        assert_eq!(vms.len(), 1);
        assert_eq!(vms[0].name, "web");
    }
}
