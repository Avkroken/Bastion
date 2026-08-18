//! Systemöversikt hämtad agentlöst över SSH — port av
//! `Sources/SSHCore/SystemProbe.swift`. Ett kombinerat kommando (last,
//! minne, disk, drifttid, OS, kärna, värdnamn, kärnantal, Docker) ger en
//! ögonblicksbild i EN round-trip; parsningen är rena funktioner
//! (sträng -> struct), testade med samma fixtures som Swift-sidan.

pub const COMMAND: &str = "echo @@LOADAVG; cat /proc/loadavg 2>/dev/null; \
echo @@UPTIME; cat /proc/uptime 2>/dev/null; \
echo @@MEM; cat /proc/meminfo 2>/dev/null; \
echo @@DF; df -kP 2>/dev/null; \
echo @@OS; cat /etc/os-release 2>/dev/null; \
echo @@KERNEL; uname -sr 2>/dev/null; \
echo @@HOST; cat /proc/sys/kernel/hostname 2>/dev/null; \
echo @@NPROC; nproc 2>/dev/null; \
echo @@DOCKER; docker ps --format '{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}' 2>/dev/null; \
echo @@TEMP; for z in /sys/class/thermal/thermal_zone*; do \
[ -r \"$z/temp\" ] && echo \"$(cat \"$z/type\" 2>/dev/null)|$(cat \"$z/temp\" 2>/dev/null)\"; done 2>/dev/null; \
echo @@IP; ip -o addr show scope global 2>/dev/null; \
echo @@IPFALLBACK; hostname -I 2>/dev/null; \
echo @@KEYS; ssh-keygen -l -f \"$HOME/.ssh/authorized_keys\" 2>/dev/null; \
echo @@WHO; who 2>/dev/null; \
echo @@END";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadAverage {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemoryInfo {
    pub total_bytes: i64,
    pub available_bytes: i64,
}

impl MemoryInfo {
    pub fn used_bytes(&self) -> i64 {
        (self.total_bytes - self.available_bytes).max(0)
    }

    pub fn used_fraction(&self) -> f64 {
        if self.total_bytes > 0 {
            self.used_bytes() as f64 / self.total_bytes as f64
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiskUsage {
    pub filesystem: String,
    pub mount: String,
    pub size_bytes: i64,
    pub used_bytes: i64,
    pub available_bytes: i64,
    pub capacity_percent: i32,
}

/// En temperaturgivare ur `/sys/class/thermal`.
///
/// Sysfs och inte `sensors`: lm-sensors är sällan installerat på en
/// server, och VISION säger uttryckligen "allt via SSH, ingen agent
/// krävs". Kärnan exponerar zonerna utan att något behöver installeras.
#[derive(Debug, Clone, PartialEq)]
pub struct Temperature {
    /// Zonens typ, t.ex. `x86_pkg_temp`, `coretemp` eller `cpu-thermal`.
    pub label: String,
    pub celsius: f64,
}

/// En IP-adress på ett gränssnitt.
#[derive(Debug, Clone, PartialEq)]
pub struct IpAddress {
    pub interface: String,
    /// Med prefixlängd, som `ip` skriver den: `192.168.1.10/24`.
    pub address: String,
    pub is_ipv6: bool,
}

/// En nyckel i den inloggade användarens `authorized_keys`.
///
/// Alltså vem som KAN logga in på kontot — inte värdens egna värdnycklar.
/// Det är den frågan som hör hemma bredvid "aktiva användare" i en
/// översikt: vilka har access, och vilka är inne just nu.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthorizedKey {
    pub bits: i32,
    /// `SHA256:…`
    pub fingerprint: String,
    /// Kommentaren i nyckelraden, oftast `användare@maskin`. Tom när
    /// nyckeln saknar en.
    pub comment: String,
    /// `ED25519`, `RSA`, `ECDSA` …
    pub algorithm: String,
}

/// En inloggad användare, ur `who`.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveUser {
    pub user: String,
    pub tty: String,
    pub since: String,
    /// Varifrån sessionen kommer, när `who` rapporterar det.
    pub from: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
}

impl DockerContainer {
    /// Härleds ur statustexten ("Up 3 days" = igång, "Exited (0)…" = stoppad)
    /// — samma heuristik som `SystemProbe.swift`.
    pub fn is_running(&self) -> bool {
        self.status.starts_with("Up")
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemSnapshot {
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub kernel: Option<String>,
    pub cpu_count: Option<i32>,
    pub uptime_seconds: Option<f64>,
    pub load: Option<LoadAverage>,
    pub memory: Option<MemoryInfo>,
    pub disks: Vec<DiskUsage>,
    pub containers: Vec<DockerContainer>,
    pub temperatures: Vec<Temperature>,
    pub addresses: Vec<IpAddress>,
    pub authorized_keys: Vec<AuthorizedKey>,
    pub active_users: Vec<ActiveUser>,
}

impl SystemSnapshot {
    /// Rot-filsystemet, om det finns — det UI:t oftast visar först.
    /// `main.rs` listar just nu ALLA diskar rakt av (ingen "visa rot
    /// först"-sortering), så den här används bara av testerna — kvar för
    /// paritet med Swift-sidans `rootDisk`, samma motivering som
    /// `WireGuardProfileStore::get` i `wireguard.rs`.
    #[allow(dead_code)]
    pub fn root_disk(&self) -> Option<&DiskUsage> {
        self.disks.iter().find(|d| d.mount == "/")
    }
}

/// Delar `output` i `@@SEKTION`-block, precis som Swift-sidans `parse` —
/// sektionsmarkörer skiljer de sammanslagna kommandonas utdata åt eftersom
/// allt kommer tillbaka i EN sträng från en enda round-trip.
fn sections(output: &str) -> std::collections::HashMap<String, Vec<String>> {
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut current = String::new();
    // `filter(!is_empty)` motsvarar Swifts `split(whereSeparator:)`, som
    // (till skillnad från Rusts `str::split`) utelämnar tomma delsträngar
    // som standard — annars hade en avslutande radbrytning gett en
    // spöklinje i den sist öppnade sektionen.
    for line in output.split(['\n', '\r']).filter(|l| !l.is_empty()) {
        if let Some(name) = line.strip_prefix("@@") {
            current = name.to_string();
            map.entry(current.clone()).or_default();
        } else if !current.is_empty() {
            map.entry(current.clone()).or_default().push(line.to_string());
        }
    }
    map
}

pub fn parse(output: &str) -> SystemSnapshot {
    let sections = sections(output);
    let first = |key: &str| sections.get(key).and_then(|v| v.first()).cloned();

    SystemSnapshot {
        hostname: first("HOST").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        os: parse_os(sections.get("OS").map(Vec::as_slice).unwrap_or(&[])),
        kernel: first("KERNEL").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        cpu_count: first("NPROC").and_then(|s| s.trim().parse().ok()),
        uptime_seconds: first("UPTIME").and_then(|s| s.split(' ').next().and_then(|p| p.parse().ok())),
        load: parse_load(first("LOADAVG").as_deref()),
        memory: parse_memory(sections.get("MEM").map(Vec::as_slice).unwrap_or(&[])),
        disks: parse_disks(sections.get("DF").map(Vec::as_slice).unwrap_or(&[])),
        containers: parse_docker(sections.get("DOCKER").map(Vec::as_slice).unwrap_or(&[])),
        temperatures: parse_temperatures(sections.get("TEMP").map(Vec::as_slice).unwrap_or(&[])),
        addresses: {
            // `ip` (iproute2) saknas på minimala images — uppmätt i en
            // Ubuntu-container 2026-08-18, där sektionen blev tyst tom och
            // såg ut som "värden har inga adresser". `hostname -I` finns
            // nästan överallt och används först när `ip` inte gav något.
            let primary = parse_addresses(sections.get("IP").map(Vec::as_slice).unwrap_or(&[]));
            if primary.is_empty() {
                parse_fallback_addresses(sections.get("IPFALLBACK").map(Vec::as_slice).unwrap_or(&[]))
            } else {
                primary
            }
        },
        authorized_keys: parse_authorized_keys(sections.get("KEYS").map(Vec::as_slice).unwrap_or(&[])),
        active_users: parse_active_users(sections.get("WHO").map(Vec::as_slice).unwrap_or(&[])),
    }
}

fn parse_load(line: Option<&str>) -> Option<LoadAverage> {
    let parts: Vec<f64> = line?.split(' ').filter_map(|p| p.parse().ok()).collect();
    if parts.len() < 3 {
        return None;
    }
    Some(LoadAverage { one: parts[0], five: parts[1], fifteen: parts[2] })
}

fn parse_memory(lines: &[String]) -> Option<MemoryInfo> {
    let mut kb: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for line in lines {
        let f: Vec<&str> = line.split(' ').filter(|s| !s.is_empty()).collect();
        if f.len() < 2 {
            continue;
        }
        let key = f[0].strip_suffix(':').unwrap_or(f[0]).to_string();
        if let Ok(v) = f[1].parse::<i64>() {
            kb.insert(key, v);
        }
    }
    let total = *kb.get("MemTotal")?;
    let avail = *kb.get("MemAvailable")?;
    Some(MemoryInfo { total_bytes: total * 1024, available_bytes: avail * 1024 })
}

fn parse_disks(lines: &[String]) -> Vec<DiskUsage> {
    let mut out = Vec::new();
    for line in lines {
        let f: Vec<&str> = line.split(' ').filter(|s| !s.is_empty()).collect();
        if f.len() < 6 || f[0] == "Filesystem" {
            continue;
        }
        let (Ok(blocks), Ok(used), Ok(avail)) = (f[1].parse::<i64>(), f[2].parse::<i64>(), f[3].parse::<i64>()) else {
            continue;
        };
        let cap = f[4].trim_end_matches('%').parse().unwrap_or(0);
        out.push(DiskUsage {
            filesystem: f[0].to_string(),
            mount: f[5..].join(" "),
            size_bytes: blocks * 1024,
            used_bytes: used * 1024,
            available_bytes: avail * 1024,
            capacity_percent: cap,
        });
    }
    out
}

fn parse_os(lines: &[String]) -> Option<String> {
    for line in lines {
        if let Some(rest) = line.strip_prefix("PRETTY_NAME=") {
            let v = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(rest);
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// `type|temp` per zon, där `temp` är MILLIGRADER.
///
/// Tusendelarna är hela fällan: ett rått `55000` in i gränssnittet ser ut
/// som en trasig sensor, inte som 55 °C. Orimliga värden filtreras bort —
/// vissa zoner rapporterar `-274000` eller nollor när de saknar en riktig
/// givare, och en temperatur under absoluta nollpunkten är inte data.
fn parse_temperatures(lines: &[String]) -> Vec<Temperature> {
    lines
        .iter()
        .filter_map(|line| {
            let (label, milli) = line.split_once('|')?;
            let celsius = milli.trim().parse::<f64>().ok()? / 1000.0;
            if !(-50.0..=200.0).contains(&celsius) {
                return None;
            }
            let label = label.trim();
            Some(Temperature {
                label: if label.is_empty() { "okänd".to_string() } else { label.to_string() },
                celsius,
            })
        })
        .collect()
}

/// `ip -o addr show scope global` ger rader som
/// `2: eth0    inet 192.168.1.10/24 brd … scope global eth0\       valid_lft …`
///
/// `scope global` i kommandot gör att loopback och link-local aldrig når
/// hit — de säger ingenting om hur maskinen nås utifrån, vilket är hela
/// frågan posten svarar på.
fn parse_addresses(lines: &[String]) -> Vec<IpAddress> {
    lines
        .iter()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            // index 0 är "2:", 1 är gränssnittet, 2 är inet/inet6, 3 adressen
            if f.len() < 4 {
                return None;
            }
            let is_ipv6 = match f[2] {
                "inet" => false,
                "inet6" => true,
                _ => return None,
            };
            Some(IpAddress {
                interface: f[1].to_string(),
                address: f[3].to_string(),
                is_ipv6,
            })
        })
        .collect()
}

/// `hostname -I` ger adresserna mellanslagsseparerade och UTAN
/// gränssnitt — det är priset för att fungera på en värd som saknar
/// iproute2. Gränssnittet blir `okänt` snarare än påhittat.
fn parse_fallback_addresses(lines: &[String]) -> Vec<IpAddress> {
    lines
        .iter()
        .flat_map(|line| line.split_whitespace())
        .filter(|a| !a.is_empty())
        .map(|address| IpAddress {
            interface: "okänt".to_string(),
            // Ingen prefixlängd att gå på här, så typen avgörs av kolon —
            // en IPv6-adress innehåller alltid minst ett, en IPv4 aldrig.
            is_ipv6: address.contains(':'),
            address: address.to_string(),
        })
        .collect()
}

/// `ssh-keygen -l -f authorized_keys` ger
/// `256 SHA256:abc… anders@laptop (ED25519)`
///
/// Kommentaren kan innehålla mellanslag och algoritmen står inom
/// parentes SIST — därför plockas båda ändarna först och kommentaren blir
/// vad som blir kvar i mitten. Ett fast index hade brutit på varje nyckel
/// vars kommentar inte är ett enda ord.
fn parse_authorized_keys(lines: &[String]) -> Vec<AuthorizedKey> {
    lines
        .iter()
        .filter_map(|line| {
            let line = line.trim();
            let (algorithm, rest) = match line.rfind('(') {
                Some(open) if line.ends_with(')') => {
                    (line[open + 1..line.len() - 1].to_string(), line[..open].trim())
                }
                _ => return None,
            };
            let mut parts = rest.splitn(3, char::is_whitespace);
            let bits: i32 = parts.next()?.parse().ok()?;
            let fingerprint = parts.next()?.to_string();
            let comment = parts.next().unwrap_or("").trim().to_string();
            Some(AuthorizedKey { bits, fingerprint, comment, algorithm })
        })
        .collect()
}

/// `who` ger `anders   pts/0        2026-08-18 19:14 (192.168.1.5)`
///
/// Ursprunget står inom parentes och saknas för lokala inloggningar —
/// därför `Option`, inte en tom sträng. Skillnaden mellan "loggade in
/// från 192.168.1.5" och "sitter vid maskinen" är värd att behålla.
fn parse_active_users(lines: &[String]) -> Vec<ActiveUser> {
    lines
        .iter()
        .filter_map(|line| {
            let line = line.trim();
            let (rest, from) = match (line.rfind('('), line.ends_with(')')) {
                (Some(open), true) => (
                    line[..open].trim(),
                    Some(line[open + 1..line.len() - 1].to_string()),
                ),
                _ => (line, None),
            };
            let f: Vec<&str> = rest.split_whitespace().collect();
            if f.len() < 3 {
                return None;
            }
            Some(ActiveUser {
                user: f[0].to_string(),
                tty: f[1].to_string(),
                since: f[2..].join(" "),
                from,
            })
        })
        .collect()
}

fn parse_docker(lines: &[String]) -> Vec<DockerContainer> {
    lines
        .iter()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('|').collect();
            if f.len() < 4 {
                return None;
            }
            Some(DockerContainer {
                id: f[0].to_string(),
                name: f[1].to_string(),
                image: f[2].to_string(),
                status: f[3].to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtur byggd på verklig utdata från en Ubuntu-maskin — samma fixtur
    // som `Tests/SSHCoreTests/SystemProbeTests.swift` använder, rad för
    // rad avskriven.
    const FIXTURE: &str = "@@LOADAVG\n\
2.53 1.86 1.81 3/1005 1015672\n\
@@UPTIME\n\
259335.25 2634310.61\n\
@@MEM\n\
MemTotal:       15244848 kB\n\
MemFree:          680100 kB\n\
MemAvailable:   10814800 kB\n\
@@DF\n\
Filesystem          1024-blocks       Used   Available Capacity Mounted on\n\
tmpfs                   3048972       2472     3046500       1% /run\n\
/dev/nvme0n1p2        102626232   22509936    74857032      24% /\n\
tmpfs                   7622424          0     7622424       0% /dev/shm\n\
@@OS\n\
PRETTY_NAME=\"Ubuntu 26.04 LTS\"\n\
NAME=\"Ubuntu\"\n\
@@KERNEL\n\
Linux 7.0.0-27-generic\n\
@@HOST\n\
mp100\n\
@@NPROC\n\
12\n\
@@DOCKER\n\
a1b2c3d4e5f6|plex|linuxserver/plex:latest|Up 3 days\n\
f6e5d4c3b2a1|radarr|linuxserver/radarr|Up 2 hours (healthy)\n\
@@END\n";

    #[test]
    fn parses_full_snapshot() {
        let s = parse(FIXTURE);

        assert_eq!(s.load, Some(LoadAverage { one: 2.53, five: 1.86, fifteen: 1.81 }));
        assert_eq!(s.uptime_seconds, Some(259335.25));
        assert_eq!(s.kernel.as_deref(), Some("Linux 7.0.0-27-generic"));
        assert_eq!(s.hostname.as_deref(), Some("mp100"));
        assert_eq!(s.os.as_deref(), Some("Ubuntu 26.04 LTS"));
        assert_eq!(s.cpu_count, Some(12));

        let mem = s.memory.expect("minnesdata saknas");
        assert_eq!(mem.total_bytes, 15244848 * 1024);
        assert_eq!(mem.available_bytes, 10814800 * 1024);
        assert_eq!(mem.used_bytes(), (15244848 - 10814800) * 1024);

        // Rot-disken plockas ut korrekt bland flera monteringar.
        assert_eq!(s.disks.len(), 3);
        let root = s.root_disk().expect("rot-disk saknas");
        assert_eq!(root.filesystem, "/dev/nvme0n1p2");
        assert_eq!(root.capacity_percent, 24);
        assert_eq!(root.size_bytes, 102626232 * 1024);

        assert_eq!(s.containers.len(), 2);
        assert_eq!(s.containers[0].name, "plex");
        assert_eq!(s.containers[1].status, "Up 2 hours (healthy)");
    }

    #[test]
    fn missing_sections_are_none_not_a_crash() {
        // Minimal maskin: ingen docker, ingen nproc, ingen os-release.
        let minimal = "@@LOADAVG\n0.00 0.01 0.05 1/100 999\n@@MEM\nMemTotal:       1000000 kB\nMemAvailable:    500000 kB\n@@DF\n@@DOCKER\n@@END\n";
        let s = parse(minimal);
        assert_eq!(s.load.map(|l| l.one), Some(0.0));
        assert!((s.memory.expect("minnesdata saknas").used_fraction() - 0.5).abs() < 0.0001);
        assert_eq!(s.cpu_count, None);
        assert_eq!(s.os, None);
        assert!(s.root_disk().is_none());
        assert!(s.containers.is_empty());
    }

    #[test]
    fn garbage_output_yields_an_empty_snapshot() {
        let s = parse("slumpmässigt skräp utan markörer");
        assert_eq!(s.load, None);
        assert_eq!(s.memory, None);
        assert!(s.disks.is_empty());
    }

    #[test]
    fn docker_container_running_status_is_derived_from_the_status_prefix() {
        let running = DockerContainer { id: "a".into(), name: "n".into(), image: "i".into(), status: "Up 3 days".into() };
        let stopped = DockerContainer { id: "b".into(), name: "n".into(), image: "i".into(), status: "Exited (0) 2 hours ago".into() };
        assert!(running.is_running());
        assert!(!stopped.is_running());
    }
}

#[cfg(test)]
mod new_sections_tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    /// Fällan är tusendelarna: ett rått 55000 in i gränssnittet ser ut som
    /// en trasig sensor, inte som 55 grader.
    #[test]
    fn millidegrees_become_celsius_and_impossible_values_are_dropped() {
        let temps = parse_temperatures(&lines(
            "x86_pkg_temp|55000\nacpitz|27800\nokänd_zon|-274000\ntrasig|inte-ett-tal\n|41000",
        ));
        assert_eq!(temps.len(), 3);
        assert_eq!(temps[0].label, "x86_pkg_temp");
        assert_eq!(temps[0].celsius, 55.0);
        assert_eq!(temps[1].celsius, 27.8);
        assert_eq!(temps[2].label, "okänd", "tom etikett ska inte ge en tom rad");
        assert_eq!(temps[2].celsius, 41.0);
    }

    #[test]
    fn addresses_separate_ipv4_and_ipv6_and_keep_the_interface() {
        let out = lines(
            "2: eth0    inet 192.168.1.10/24 brd 192.168.1.255 scope global eth0\\       valid_lft forever\n\
             2: eth0    inet6 2001:db8::1/64 scope global \\       valid_lft forever\n\
             3: wg0     inet 10.8.0.2/32 scope global wg0\\       valid_lft forever",
        );
        let addrs = parse_addresses(&out);
        assert_eq!(addrs.len(), 3);
        assert_eq!(addrs[0].interface, "eth0");
        assert_eq!(addrs[0].address, "192.168.1.10/24");
        assert!(!addrs[0].is_ipv6);
        assert!(addrs[1].is_ipv6);
        assert_eq!(addrs[2].interface, "wg0", "tunnlar är också globala adresser");
    }

    /// Kommentaren kan innehålla mellanslag och algoritmen står SIST inom
    /// parentes. Ett fast index hade brutit på varje nyckel vars kommentar
    /// inte är ett enda ord.
    #[test]
    fn key_comments_may_contain_spaces_and_the_algorithm_is_read_from_the_end() {
        let keys = parse_authorized_keys(&lines(
            "256 SHA256:abc123 anders@laptop (ED25519)\n\
             3072 SHA256:def456 min gamla nyckel från 2019 (RSA)\n\
             256 SHA256:ghi789 no comment here (ECDSA)",
        ));
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0].bits, 256);
        assert_eq!(keys[0].fingerprint, "SHA256:abc123");
        assert_eq!(keys[0].comment, "anders@laptop");
        assert_eq!(keys[0].algorithm, "ED25519");
        assert_eq!(keys[1].comment, "min gamla nyckel från 2019");
        assert_eq!(keys[1].algorithm, "RSA");
    }

    /// Utan authorized_keys skriver ssh-keygen ingenting alls till stdout,
    /// och felet gick till /dev/null. Tomt ska ge noll nycklar, inte en
    /// halv post.
    #[test]
    fn missing_or_malformed_key_lines_yield_nothing() {
        assert!(parse_authorized_keys(&lines("")).is_empty());
        assert!(parse_authorized_keys(&lines("no such file or directory")).is_empty());
        assert!(parse_authorized_keys(&lines("256 SHA256:abc anders@laptop")).is_empty(),
                "utan algoritm inom parentes är raden inte en nyckel");
    }

    /// Skillnaden mellan "loggade in från 192.168.1.5" och "sitter vid
    /// maskinen" är värd att behålla — därför Option och inte tom sträng.
    #[test]
    fn remote_origin_is_optional_and_local_logins_have_none() {
        let users = parse_active_users(&lines(
            "anders   pts/0        2026-08-18 19:14 (192.168.1.5)\n\
             root     tty1         2026-08-18 08:02\n\
             deploy   pts/2        2026-08-18 19:20 (10.8.0.9)",
        ));
        assert_eq!(users.len(), 3);
        assert_eq!(users[0].user, "anders");
        assert_eq!(users[0].tty, "pts/0");
        assert_eq!(users[0].since, "2026-08-18 19:14");
        assert_eq!(users[0].from.as_deref(), Some("192.168.1.5"));
        assert_eq!(users[1].from, None, "lokal inloggning har inget ursprung");
        assert_eq!(users[1].since, "2026-08-18 08:02");
    }

    /// Uppmätt problem, inte påhittat: `ip` saknas på minimala images
    /// (verifierat i en Ubuntu-container 2026-08-18), och utan fallback
    /// blev IP-posten tyst tom — omöjlig att skilja från "värden har inga
    /// adresser".
    #[test]
    fn hostname_fallback_is_used_only_when_ip_gave_nothing() {
        let with_ip = "@@IP\n2: eth0    inet 10.0.0.5/24 scope global eth0\n\
@@IPFALLBACK\n192.0.2.2 198.51.100.7\n@@END";
        let snap = parse(with_ip);
        assert_eq!(snap.addresses.len(), 1, "finns ip-utdata ska fallbacken inte användas");
        assert_eq!(snap.addresses[0].interface, "eth0");

        let without_ip = "@@IP\n@@IPFALLBACK\n192.0.2.2 2001:db8::5\n@@END";
        let snap = parse(without_ip);
        assert_eq!(snap.addresses.len(), 2);
        assert_eq!(snap.addresses[0].address, "192.0.2.2");
        assert_eq!(snap.addresses[0].interface, "okänt", "gränssnittet ska inte hittas på");
        assert!(!snap.addresses[0].is_ipv6);
        assert!(snap.addresses[1].is_ipv6, "kolon avgör familjen utan prefixlängd");

        let neither = parse("@@IP\n@@IPFALLBACK\n@@END");
        assert!(neither.addresses.is_empty());
    }

    /// Alla fyra sektionerna ska överleva en full parse tillsammans med de
    /// gamla — och en värd som saknar dem ska ge tomma listor, inte fel.
    #[test]
    fn the_four_new_sections_survive_a_full_parse_and_absence_is_not_an_error() {
        let full = "@@HOST\nsrv1\n@@TEMP\nx86_pkg_temp|55000\n@@IP\n2: eth0    inet 10.0.0.5/24 scope global eth0\n\
@@KEYS\n256 SHA256:abc anders@laptop (ED25519)\n@@WHO\nanders   pts/0        2026-08-18 19:14 (10.0.0.1)\n@@END";
        let snap = parse(full);
        assert_eq!(snap.hostname.as_deref(), Some("srv1"));
        assert_eq!(snap.temperatures.len(), 1);
        assert_eq!(snap.addresses.len(), 1);
        assert_eq!(snap.authorized_keys.len(), 1);
        assert_eq!(snap.active_users.len(), 1);

        let bare = parse("@@HOST\nsrv2\n@@END");
        assert_eq!(bare.hostname.as_deref(), Some("srv2"));
        assert!(bare.temperatures.is_empty());
        assert!(bare.addresses.is_empty());
        assert!(bare.authorized_keys.is_empty());
        assert!(bare.active_users.is_empty());
    }

    /// Kommandot ska faktiskt fråga efter allt VISION räknar upp.
    #[test]
    fn the_command_asks_for_every_section_the_parser_reads() {
        for section in ["@@TEMP", "@@IP", "@@IPFALLBACK", "@@KEYS", "@@WHO"] {
            assert!(COMMAND.contains(section), "{section} saknas i COMMAND");
        }
    }
}
