//! Minimal läsare av OpenSSH:s klientkonfiguration (`~/.ssh/config`). Port
//! av `Sources/SSHCore/SSHConfig.swift`. Stöder `Host`-block med
//! jokertecken (`*`, `?`) och negation (`!`), `Include`, samt de
//! vanligaste nycklarna. Semantik enligt OpenSSH: **första värdet vinner**
//! per nyckel. `Match`-block hoppas medvetet över (ännu ej stött) — samma
//! avgränsning som Swift-sidan.

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

/// Största antal `Include`-nivåer som följs. Samma gräns som OpenSSH:s
/// egen (`readconf.c`, `MAX_READCONF_DEPTH` = 16), och av samma skäl: en
/// config som direkt eller indirekt inkluderar sig själv ska ge en
/// trunkerad läsning, inte en oändlig loop.
const MAX_INCLUDE_DEPTH: usize = 16;

impl SSHConfig {
    /// Läser en config-TEXT utan att röra filsystemet. `Include`-rader
    /// hoppas över — det finns ingen katalog att lösa dem mot. Använd
    /// [`SSHConfig::parse_file`] när filen finns på disk.
    pub fn parse(text: &str) -> SSHConfig {
        let mut entries = Vec::new();
        collect_entries(text, None, 0, &mut entries);
        SSHConfig { entries }
    }

    /// Läser en config från disk och FÖLJER `Include`-rader.
    ///
    /// Det här är skillnaden mellan att importera en modern
    /// `~/.ssh/config` och att importera ingenting alls: `Include
    /// ~/.ssh/config.d/*` är hur de flesta verktyg (1Password, Colima,
    /// OrbStack m.fl.) säger åt användaren att lägga upp sin config, och
    /// då står det inte en enda `Host`-rad i huvudfilen.
    ///
    /// Saknade filer hoppas tyst över. OpenSSH felar på en Include utan
    /// jokertecken som pekar på en fil som inte finns, men här är
    /// alternativet att en enda död sökväg gör att INGA värdar
    /// importeras — sämre för det här användningsfallet.
    pub fn parse_file(path: &std::path::Path) -> std::io::Result<SSHConfig> {
        let text = std::fs::read_to_string(path)?;
        let base = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let mut entries = Vec::new();
        collect_entries(&text, Some(&base), 0, &mut entries);
        Ok(SSHConfig { entries })
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
/// Tolkar en config-text till poster, och expanderar `Include` INLINE på
/// den plats raden stod. Att inlina är inte en förenkling utan just vad
/// OpenSSH gör: en inkluderad fils `Host`-block gäller vidare efter
/// include-punkten, precis som om innehållet stått där direkt.
fn collect_entries(
    text: &str,
    base_dir: Option<&std::path::Path>,
    depth: usize,
    out: &mut Vec<Entry>,
) {
    for raw_line in text.split(['\n', '\r']) {
        let Some((key, value)) = tokenize(raw_line) else { continue };
        match key.as_str() {
            "host" => {
                let patterns: Vec<String> = value
                    .split([' ', '\t'])
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect();
                out.push(Entry::Host(patterns));
            }
            "match" => {
                // Ej stött — tomt mönster matchar aldrig, så blockets
                // nycklar ignoreras.
                out.push(Entry::Host(Vec::new()));
            }
            "include" => {
                let Some(base_dir) = base_dir else { continue };
                if depth >= MAX_INCLUDE_DEPTH {
                    continue;
                }
                for path in resolve_include(&value, base_dir) {
                    if let Ok(nested) = std::fs::read_to_string(&path) {
                        collect_entries(&nested, Some(base_dir), depth + 1, out);
                    }
                }
            }
            _ => out.push(Entry::Setting(key, value)),
        }
    }
}

/// Löser upp en `Include`-rads sökvägar till konkreta filer.
///
/// En rad kan ange flera sökvägar separerade med blanksteg, var och en med
/// `~` och/eller jokertecken. Relativa sökvägar räknas från katalogen
/// configfilen ligger i — OpenSSH säger `~/.ssh` för användarens config,
/// vilket är samma katalog i praktiken men blir rätt även när filen ligger
/// någon annanstans (t.ex. i ett test).
///
/// Träffarna sorteras. OpenSSH läser glob-träffar i den ordning `glob(3)`
/// ger, vilket är sorterad ordning — och ordningen spelar roll, eftersom
/// första värdet vinner per nyckel.
fn resolve_include(value: &str, base_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for raw in value.split([' ', '\t']).filter(|s| !s.is_empty()) {
        let expanded = expand_tilde(raw);
        let candidate = std::path::Path::new(&expanded);
        let full = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            base_dir.join(candidate)
        };

        if !expanded.contains('*') && !expanded.contains('?') {
            out.push(full);
            continue;
        }

        // Jokertecken hanteras bara i SISTA segmentet, som i OpenSSH:s
        // egna exempel (`Include ~/.ssh/config.d/*`). Ett mönster mitt i
        // sökvägen är sällsynt nog att inte vara värt en egen
        // katalogtraversering.
        let (Some(dir), Some(pattern)) = (full.parent(), full.file_name()) else {
            continue;
        };
        let pattern = pattern.to_string_lossy().into_owned();
        let Ok(read) = std::fs::read_dir(dir) else { continue };
        let mut matches: Vec<std::path::PathBuf> = read
            .flatten()
            .filter(|e| glob(&pattern, &e.file_name().to_string_lossy()))
            .map(|e| e.path())
            .collect();
        matches.sort();
        out.extend(matches);
    }
    out
}

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

    /// Utan `Include` importeras NOLL värdar ur en modern config, utan att
    /// något ser trasigt ut. Det är hela poängen med den här funktionen,
    /// så testet speglar exakt det upplägg verktyg som 1Password och
    /// OrbStack instruerar användaren att skapa: en huvudfil som bara
    /// pekar vidare, och inte en enda `Host`-rad i den.
    #[test]
    fn hosts_behind_an_include_are_found_and_would_not_be_without_it() {
        let dir = temp_config_dir();
        std::fs::create_dir_all(dir.join("config.d")).unwrap();
        std::fs::write(dir.join("config"), "Include config.d/work\n").unwrap();
        std::fs::write(
            dir.join("config.d/work"),
            "Host kund\n  HostName kund.example\n  User anders\n  Port 2222\n",
        )
        .unwrap();

        let config = SSHConfig::parse_file(&dir.join("config")).unwrap();
        assert_eq!(config.host_aliases(), vec!["kund".to_string()]);
        let resolved = config.resolve("kund");
        assert_eq!(resolved.host_name, "kund.example");
        assert_eq!(resolved.user.as_deref(), Some("anders"));
        assert_eq!(resolved.port, 2222);

        // Kontrollen: samma text utan filsystemet ger ingenting. Utan den
        // här raden bevisar testet inte att det var Include som gjorde
        // jobbet.
        let text = std::fs::read_to_string(dir.join("config")).unwrap();
        assert!(
            SSHConfig::parse(&text).host_aliases().is_empty(),
            "utan Include-upplösning finns ingen värd i huvudfilen — det är felet som fixas"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    /// `Include ~/.ssh/config.d/*` är det vanligaste sättet raden skrivs.
    /// Ordningen måste vara sorterad: första värdet vinner per nyckel, så
    /// en ostabil läsordning skulle ge olika resultat mellan körningar på
    /// samma filer.
    #[test]
    fn a_wildcard_include_reads_every_match_in_sorted_order() {
        let dir = temp_config_dir();
        std::fs::create_dir_all(dir.join("config.d")).unwrap();
        std::fs::write(dir.join("config"), "Include config.d/*\n").unwrap();
        std::fs::write(dir.join("config.d/10-a"), "Host alfa\n  User a\n").unwrap();
        std::fs::write(dir.join("config.d/20-b"), "Host beta\n  User b\n").unwrap();
        std::fs::write(dir.join("config.d/30-c"), "Host gamma\n  User c\n").unwrap();

        let config = SSHConfig::parse_file(&dir.join("config")).unwrap();
        assert_eq!(
            config.host_aliases(),
            vec!["alfa".to_string(), "beta".to_string(), "gamma".to_string()]
        );
        std::fs::remove_dir_all(dir).ok();
    }

    /// En config som inkluderar sig själv ska ge en trunkerad läsning, inte
    /// hänga sig. Utan djupgränsen är det här inte ett långsamt test utan
    /// ett test som aldrig återvänder.
    #[test]
    fn a_self_including_config_stops_instead_of_looping_forever() {
        let dir = temp_config_dir();
        std::fs::write(dir.join("config"), "Include config\nHost slut\n  User a\n").unwrap();

        let config = SSHConfig::parse_file(&dir.join("config")).unwrap();
        // Värden finns kvar — läsningen trunkerades, den kraschade inte.
        assert!(config.host_aliases().contains(&"slut".to_string()));
        std::fs::remove_dir_all(dir).ok();
    }

    /// En Include som pekar på en fil som inte finns får inte ta med sig
    /// resten av configen i fallet. Ett kvarglömt verktyg som avinstallerats
    /// lämnar precis en sådan rad efter sig.
    #[test]
    fn a_missing_include_target_does_not_discard_the_rest_of_the_config() {
        let dir = temp_config_dir();
        std::fs::write(
            dir.join("config"),
            "Include config.d/finns-inte\nHost kvar\n  HostName kvar.example\n  User a\n",
        )
        .unwrap();

        let config = SSHConfig::parse_file(&dir.join("config")).unwrap();
        assert_eq!(config.host_aliases(), vec!["kvar".to_string()]);
        std::fs::remove_dir_all(dir).ok();
    }

    /// Poster ur en inkluderad fil måste hamna på include-radens PLATS, inte
    /// sist. Annars ändras vilket värde som vinner — OpenSSH tar det första,
    /// så en omflyttning byter tyst ut användarens inställningar.
    #[test]
    fn included_settings_land_where_the_include_line_stood() {
        let dir = temp_config_dir();
        std::fs::write(
            dir.join("config"),
            "Host server\n  User fran-huvudfilen\nInclude senare\n",
        )
        .unwrap();
        std::fs::write(dir.join("senare"), "Host server\n  User fran-included\n").unwrap();

        let config = SSHConfig::parse_file(&dir.join("config")).unwrap();
        assert_eq!(
            config.resolve("server").user.as_deref(),
            Some("fran-huvudfilen"),
            "första värdet vinner — den inkluderade filen stod EFTER och ska inte skriva över"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    /// Hela vägen fram: från fil på disk till värdar i databasen.
    #[test]
    fn importing_from_a_file_stores_hosts_that_only_exist_behind_an_include() {
        let dir = temp_config_dir();
        std::fs::create_dir_all(dir.join("config.d")).unwrap();
        std::fs::write(dir.join("config"), "Include config.d/*\n").unwrap();
        std::fs::write(
            dir.join("config.d/hosts"),
            "Host bakom-include\n  HostName b.example\n  User anders\n",
        )
        .unwrap();

        let mut store = crate::host::HostStore::open(dir.join("hosts.json")).unwrap();
        assert_eq!(store.import_ssh_config_file(&dir.join("config")).unwrap(), 1);
        assert_eq!(store.all()[0].alias, "bakom-include");
        // Omimport ska fortfarande inte skapa dubbletter.
        assert_eq!(store.import_ssh_config_file(&dir.join("config")).unwrap(), 0);
        std::fs::remove_dir_all(dir).ok();
    }

    fn temp_config_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bastion-include-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
