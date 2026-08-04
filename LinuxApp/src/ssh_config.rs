//! Minimal läsare av OpenSSH:s klientkonfiguration (`~/.ssh/config`). Port
//! av `Sources/SSHCore/SSHConfig.swift`. Stöder `Host`-block med
//! jokertecken (`*`, `?`) och negation (`!`), samt de vanligaste nycklarna.
//! Semantik enligt OpenSSH: **första värdet vinner** per nyckel. `Match`-
//! block hoppas medvetet över (ännu ej stött) — samma avgränsning som
//! Swift-sidan.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedHost {
    pub host_name: String,
    pub user: Option<String>,
    pub port: i64,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
}

enum Entry {
    Host(Vec<String>),
    Setting(String, String),
}

pub struct SSHConfig {
    entries: Vec<Entry>,
}

impl SSHConfig {
    pub fn parse(text: &str) -> SSHConfig {
        let mut entries = Vec::new();
        for raw_line in text.split(['\n', '\r']) {
            let Some((key, value)) = tokenize(raw_line) else { continue };
            match key.as_str() {
                "host" => {
                    let patterns: Vec<String> = value
                        .split([' ', '\t'])
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect();
                    entries.push(Entry::Host(patterns));
                }
                "match" => {
                    // Ej stött — tomt mönster matchar aldrig, så blockets
                    // nycklar ignoreras.
                    entries.push(Entry::Host(Vec::new()));
                }
                _ => entries.push(Entry::Setting(key, value)),
            }
        }
        SSHConfig { entries }
    }

    /// Konkreta värdalias (inte jokertecken/negation) i den ordning de
    /// står — underlag för att importera värdar till host-databasen.
    pub fn host_aliases(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for entry in &self.entries {
            let Entry::Host(patterns) = entry else { continue };
            for p in patterns {
                if !p.contains('*') && !p.contains('?') && !p.starts_with('!') && seen.insert(p.clone()) {
                    out.push(p.clone());
                }
            }
        }
        out
    }

    /// Slår upp ett alias. Nycklar före första `Host` är globala (gäller alla).
    pub fn resolve(&self, alias: &str) -> ResolvedHost {
        let mut found: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut active = true; // global sektion tills första Host/Match
        for entry in &self.entries {
            match entry {
                Entry::Host(patterns) => active = host_matches(patterns, alias),
                Entry::Setting(key, value) => {
                    if active {
                        found.entry(key.clone()).or_insert_with(|| value.clone());
                    }
                }
            }
        }
        ResolvedHost {
            host_name: found.get("hostname").cloned().unwrap_or_else(|| alias.to_string()),
            user: found.get("user").cloned(),
            port: found.get("port").and_then(|p| p.parse().ok()).unwrap_or(22),
            identity_file: found.get("identityfile").map(|p| expand_tilde(p)),
            proxy_jump: found.get("proxyjump").cloned(),
        }
    }
}

/// Byter ut ett ledande `~` mot hemkatalogen — samma effekt som Swifts
/// `NSString.expandingTildeInPath`, men bara för det enda fallet
/// `IdentityFile` faktiskt använder (`~/...`), inte den fulla `~user/...`-
/// varianten.
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest).to_string_lossy().into_owned();
    }
    path.to_string()
}

/// Delar en rad i (nyckel-gemener, värde). Stöder `Key Value`, `Key=Value`,
/// `Key = Value` och citerade värden. Returnerar `None` för tomma/
/// kommentarrader.
fn tokenize(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let Some(sep) = trimmed.find([' ', '\t', '=']) else {
        return Some((trimmed.to_lowercase(), String::new()));
    };
    let key = trimmed[..sep].to_lowercase();
    let mut value = trimmed[sep + 1..].trim_matches([' ', '\t', '=']).to_string();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value = value[1..value.len() - 1].to_string();
    }
    Some((key, value))
}

/// En värd matchar om minst ett positivt mönster matchar och inget
/// negerat gör det.
fn host_matches(patterns: &[String], host: &str) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let mut matched = false;
    for pattern in patterns {
        if let Some(negated) = pattern.strip_prefix('!') {
            if glob(negated, host) {
                return false;
            }
        } else if glob(pattern, host) {
            matched = true;
        }
    }
    matched
}

/// Jokertecken-matchning med `*` (noll+ tecken) och `?` (exakt ett
/// tecken) — samma iterativa algoritm som Swift-sidan (ingen regex).
pub fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti): (i64, i64) = (0, 0);
    let (mut star, mut mark): (i64, i64) = (-1, 0);
    while (ti as usize) < t.len() {
        if (pi as usize) < p.len() && (p[pi as usize] == '?' || p[pi as usize] == t[ti as usize]) {
            pi += 1;
            ti += 1;
        } else if (pi as usize) < p.len() && p[pi as usize] == '*' {
            star = pi;
            mark = ti;
            pi += 1;
        } else if star != -1 {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while (pi as usize) < p.len() && p[pi as usize] == '*' {
        pi += 1;
    }
    pi as usize == p.len()
}

/// Bygger värdar ur en `~/.ssh/config`. Varje konkret `Host`-alias blir en
/// post med upplösta HostName/User/Port/IdentityFile. Alias utan
/// användare hoppas över (kan inte anslutas ändå).
pub fn imported_hosts(config: &SSHConfig) -> Vec<crate::host::Host> {
    config
        .host_aliases()
        .into_iter()
        .filter_map(|alias| {
            let r = config.resolve(&alias);
            let user = r.user?;
            if user.is_empty() {
                return None;
            }
            let mut host = crate::host::Host::new(alias, r.host_name, user);
            host.port = r.port;
            host.auth = match r.identity_file {
                Some(path) => crate::host::HostAuth::KeyFile(path),
                None => crate::host::HostAuth::AgentDefault,
            };
            Some(host)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Idiomatisk OpenSSH: specifika block först, catch-all sist ("första vinner").
    const SAMPLE: &str = "Host web prod-web
    HostName 10.0.0.5
    User deploy
    Port 2222
    IdentityFile ~/.ssh/deploy_ed25519

Host *.internal
    User admin

Host bastion-*
    ProxyJump jump.example.com

Host !secret *
    User fallback
";

    #[test]
    fn exact_alias_with_all_fields() {
        let r = SSHConfig::parse(SAMPLE).resolve("web");
        assert_eq!(r.host_name, "10.0.0.5");
        assert_eq!(r.user.as_deref(), Some("deploy"));
        assert_eq!(r.port, 2222);
        assert_eq!(r.identity_file, Some(expand_tilde("~/.ssh/deploy_ed25519")));
    }

    #[test]
    fn second_pattern_on_same_host_line() {
        assert_eq!(SSHConfig::parse(SAMPLE).resolve("prod-web").host_name, "10.0.0.5");
    }

    #[test]
    fn wildcard_suffix() {
        let r = SSHConfig::parse(SAMPLE).resolve("db1.internal");
        assert_eq!(r.user.as_deref(), Some("admin")); // *.internal matchar först
        assert_eq!(r.host_name, "db1.internal"); // ingen HostName -> aliaset
    }

    #[test]
    fn wildcard_prefix_and_proxy_jump() {
        assert_eq!(
            SSHConfig::parse(SAMPLE).resolve("bastion-eu").proxy_jump.as_deref(),
            Some("jump.example.com")
        );
    }

    #[test]
    fn first_value_wins() {
        // "web" matchar både sitt eget block (User deploy) och "!secret *"
        // (User fallback). Första vinner.
        assert_eq!(SSHConfig::parse(SAMPLE).resolve("web").user.as_deref(), Some("deploy"));
    }

    #[test]
    fn negation_excludes() {
        // "secret" exkluderas av "!secret *" -> matchar inget block -> ingen User.
        let r = SSHConfig::parse(SAMPLE).resolve("secret");
        assert!(r.user.is_none());
        assert_eq!(r.host_name, "secret");
    }

    #[test]
    fn unknown_alias_hits_catch_all() {
        let r = SSHConfig::parse(SAMPLE).resolve("random");
        assert_eq!(r.host_name, "random");
        assert_eq!(r.user.as_deref(), Some("fallback")); // "!secret *" catch-all
        assert_eq!(r.port, 22);
    }

    #[test]
    fn equals_and_spaced_syntax() {
        let cfg = SSHConfig::parse("Host x\n  HostName=1.2.3.4\n  Port = 2200");
        let r = cfg.resolve("x");
        assert_eq!(r.host_name, "1.2.3.4");
        assert_eq!(r.port, 2200);
    }

    #[test]
    fn test_glob() {
        assert!(glob("*.internal", "a.internal"));
        assert!(glob("bastion-*", "bastion-eu-1"));
        assert!(glob("h??t", "host"));
        assert!(!glob("h??t", "hot"));
        assert!(!glob("*.internal", "internal"));
        assert!(glob("*", "anything"));
    }

    const IMPORT_CONFIG: &str = "Host web prod-web
    HostName 10.0.0.5
    User deploy
    Port 2222
    IdentityFile ~/.ssh/deploy_ed25519

Host nas
    HostName 10.0.0.2
    User root

Host *.internal
    User admin

Host nouser
    HostName 10.0.0.9
";

    #[test]
    fn host_aliases_skip_wildcards() {
        assert_eq!(
            SSHConfig::parse(IMPORT_CONFIG).host_aliases(),
            vec!["web", "prod-web", "nas", "nouser"]
        );
    }

    #[test]
    fn imported_hosts_skips_users_and_wildcards() {
        let hosts = imported_hosts(&SSHConfig::parse(IMPORT_CONFIG));
        // "nouser" saknar User -> hoppas över; *.internal är jokertecken.
        let mut aliases: Vec<&str> = hosts.iter().map(|h| h.alias.as_str()).collect();
        aliases.sort();
        assert_eq!(aliases, vec!["nas", "prod-web", "web"]);

        let web = hosts.iter().find(|h| h.alias == "web").unwrap();
        assert_eq!(web.host_name, "10.0.0.5");
        assert_eq!(web.user, "deploy");
        assert_eq!(web.port, 2222);
        assert_eq!(web.auth, crate::host::HostAuth::KeyFile(expand_tilde("~/.ssh/deploy_ed25519")));

        let nas = hosts.iter().find(|h| h.alias == "nas").unwrap();
        assert_eq!(nas.auth, crate::host::HostAuth::AgentDefault);
    }

    #[test]
    fn import_skips_duplicates_on_reimport() {
        let dir = std::env::temp_dir().join(format!("bastion-sshconfig-test-{}", uuid::Uuid::new_v4()));
        let mut store = crate::host::HostStore::open(dir.join("hosts.json")).unwrap();
        assert_eq!(store.import_ssh_config(IMPORT_CONFIG).unwrap(), 3);
        assert_eq!(store.import_ssh_config(IMPORT_CONFIG).unwrap(), 0); // re-import lägger inte till igen
        assert_eq!(store.all().len(), 3);
        std::fs::remove_dir_all(dir).ok();
    }
}
