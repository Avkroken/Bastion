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
