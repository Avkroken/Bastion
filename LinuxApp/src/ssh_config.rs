//! Minimal läsare av OpenSSH:s klientkonfiguration (`~/.ssh/config`). Port
//! av `Sources/SSHCore/SSHConfig.swift`. Stöder `Host`-block med
//! jokertecken (`*`, `?`) och negation (`!`), `Include`, samt de
//! vanligaste nycklarna. Semantik enligt OpenSSH: **första värdet vinner**
//! per nyckel. `Match` stöds för de kriterier som går att avgöra utan en
//! pågående anslutning (`all`, `host`); allt annat lämnar blocket
//! inaktivt — se [`match_is_active`].

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedHost {
    pub host_name: String,
    pub user: Option<String>,
    pub port: i64,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
    /// OpenSSH:s `ForwardAgent`. Bara ett uttryckligt ja räknas — allt
    /// annat, inklusive nyckelns frånvaro, är nej. Att gissa fel åt det
    /// hållet vore att slå på agentvidarebefordran åt någon som inte bett
    /// om det, och då kan vem som helst med root på fjärrvärden använda
    /// deras nycklar så länge sessionen lever.
    pub forward_agent: bool,
    /// OpenSSH:s `RemoteCommand` — kommandot som körs direkt efter
    /// anslutning. Motsvarar `Host::startup_command`.
    pub remote_command: Option<String>,
}

enum Entry {
    Host(Vec<String>),
    /// Ett `Match`-block, med kriterieraden bevarad rå. Utvärderas först i
    /// `resolve`, eftersom `host`-kriteriet beror på vilket alias som slås
    /// upp — till skillnad från `Host`, vars mönster står i posten.
    Match(String),
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
                Entry::Match(criteria) => active = match_is_active(criteria, alias),
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
            forward_agent: found
                .get("forwardagent")
                .is_some_and(|v| v.eq_ignore_ascii_case("yes")),
            remote_command: found.get("remotecommand").cloned(),
        }
    }
}

/// Plockar ut värdaliaset ur ett `ProxyJump`-värde.
///
/// Syntaxen är `[user@]host[:port]`, och flera hopp kan anges
/// kommaseparerat. Bara FÖRSTA hoppet returneras — `HostStore::resolve_jump`
/// avvisar ändå kedjor längre än ett hopp, så att importera en kedja skulle
/// skapa en koppling som inte går att använda.
///
/// `None` när värdet är tomt eller `none` (OpenSSH:s sätt att stänga av ett
/// ärvt `ProxyJump`).
pub fn proxy_jump_alias(value: &str) -> Option<String> {
    let first = value.split(',').next()?.trim();
    if first.is_empty() || first.eq_ignore_ascii_case("none") {
        return None;
    }
    let without_user = first.rsplit('@').next().unwrap_or(first);
    // IPv6-literaler skrivs `[::1]:22` — allt före den avslutande
    // klammern hör till adressen, inte till porten.
    let host = if let Some(end) = without_user.find(']') {
        &without_user[..=end]
    } else {
        without_user.split(':').next().unwrap_or(without_user)
    };
    if host.is_empty() { None } else { Some(host.to_string()) }
}

/// Avgör om ett `Match`-blocks kriterier gäller för `alias`.
///
/// OpenSSH kräver att ALLA kriterier på raden är uppfyllda. Här kan bara
/// två av dem avgöras: `all` (alltid) och `host <mönster>` (samma
/// jokertecken- och negationsregler som `Host`). Resten — `exec`, `user`,
/// `originalhost`, `localuser`, `tagged`, `final`, `canonical` — beror på
/// en pågående anslutning, en kommandokörning eller en andra
/// upplösningsomgång, inget av det finns här.
///
/// Ett okänt eller oavgörbart kriterium gör blocket INAKTIVT, aldrig
/// aktivt. Det är den enda riktning som är säker: ett block som felaktigt
/// hoppas över ger samma resultat som innan `Match` stöddes alls, medan ett
/// block som felaktigt aktiveras tyst byter ut användarens värdnamn,
/// användare eller nyckel mot någon annans.
///
/// `Match exec "..."` kommer aldrig att köras härifrån. Att köra ett
/// godtyckligt skalkommando för att avgöra en konfigurationsrad är inte en
/// funktion som saknas, det är en vi inte vill ha.
fn match_is_active(criteria: &str, alias: &str) -> bool {
    // Delas BARA på blanksteg här. Ett `host`-kriterium tar en
    // kommaseparerad mönsterlista (`Match host *.internal,!hemlig.internal`),
    // och delas kommatecknen redan här blir listans andra mönster ett eget,
    // okänt kriterium — vilket tyst gjorde varje negation till en
    // avaktivering av hela blocket.
    let tokens: Vec<&str> = criteria
        .split([' ', '\t'])
        .filter(|s| !s.is_empty())
        .collect();
    if tokens.is_empty() {
        return false;
    }

    let mut i = 0;
    let mut matched_something = false;
    while i < tokens.len() {
        match tokens[i].to_lowercase().as_str() {
            "all" => {
                matched_something = true;
                i += 1;
            }
            "host" => {
                // Kriteriet tar ett argument. Saknas det är raden trasig.
                let Some(patterns) = tokens.get(i + 1) else { return false };
                let patterns: Vec<String> = patterns.split(',').map(String::from).collect();
                if !host_matches(&patterns, alias) {
                    return false;
                }
                matched_something = true;
                i += 2;
            }
            // Allt annat: vi kan inte avgöra det, alltså gäller blocket inte.
            _ => return false,
        }
    }
    matched_something
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
            "match" => out.push(Entry::Match(value)),
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
            // Fälten finns redan i `Host` — importen fyllde dem bara aldrig
            // i, så en användare som konfigurerat dem i ssh-config fick dem
            // tyst bortkastade och undrade varför värden betedde sig
            // annorlunda i Bastion än i `ssh`.
            host.forward_agent = r.forward_agent;
            host.startup_command = r.remote_command.filter(|c| !c.is_empty());
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

    // MARK: Match

    /// `Match host` är den enda formen som går att avgöra utan en pågående
    /// anslutning, och den beter sig som `Host` — samma jokertecken, samma
    /// negation. Tidigare ignorerades hela blocket, så inställningarna i
    /// det försvann tyst.
    #[test]
    fn match_host_applies_its_settings_just_like_a_host_block() {
        let config = SSHConfig::parse(
            "Match host *.internal\n  User admin\n  Port 2200\n\nHost *\n  User fallback\n",
        );
        let inner = config.resolve("db.internal");
        assert_eq!(inner.user.as_deref(), Some("admin"));
        assert_eq!(inner.port, 2200);

        let outer = config.resolve("db.example.com");
        assert_eq!(outer.user.as_deref(), Some("fallback"), "blocket ska inte gälla utanför mönstret");
        assert_eq!(outer.port, 22);
    }

    /// `Match all` gäller alltid — men bara EFTER sin egen rad, så det
    /// fungerar som en catch-all i slutet av filen.
    #[test]
    fn match_all_applies_to_every_alias() {
        let config = SSHConfig::parse("Match all\n  User alla\n");
        assert_eq!(config.resolve("vadsomhelst").user.as_deref(), Some("alla"));
    }

    /// Kärnan i avgränsningen. Ett kriterium vi inte kan avgöra måste göra
    /// blocket INAKTIVT, aldrig aktivt — ett felaktigt aktiverat block byter
    /// tyst ut användarens värdnamn eller nyckel mot någon annans, medan ett
    /// felaktigt överhoppat block bara ger samma resultat som innan `Match`
    /// stöddes.
    ///
    /// `exec` är det viktigaste fallet: att köra ett godtyckligt skalkommando
    /// för att avgöra en konfigurationsrad är inte en funktion som saknas.
    #[test]
    fn criteria_we_cannot_evaluate_leave_the_block_inactive() {
        for criteria in [
            "exec \"test -f /tmp/x\"",
            "user root",
            "originalhost jump",
            "localuser anders",
            "final",
            "canonical",
            "tagged arbete",
        ] {
            let config = SSHConfig::parse(&format!(
                "Match {criteria}\n  User skulle-inte-synas\n\nHost *\n  User riktig\n"
            ));
            assert_eq!(
                config.resolve("nagon-vard").user.as_deref(),
                Some("riktig"),
                "kriteriet {criteria:?} går inte att avgöra och blocket ska då inte gälla"
            );
        }
    }

    /// Alla kriterier på raden måste hålla, precis som i OpenSSH. Står ett
    /// avgörbart och ett oavgörbart kriterium tillsammans räcker det inte att
    /// det första stämmer.
    #[test]
    fn every_criterion_on_the_line_must_hold_not_just_the_first() {
        let config = SSHConfig::parse(
            "Match host server user root\n  Port 9999\n\nHost *\n  User a\n",
        );
        assert_eq!(
            config.resolve("server").port, 22,
            "host stämmer men user går inte att avgöra — blocket gäller inte"
        );
    }

    /// Negation fungerar i `Match host` precis som i `Host`.
    #[test]
    fn match_host_supports_negation() {
        let config = SSHConfig::parse(
            "Match host *.internal,!secret.internal\n  User admin\n\nHost *\n  User a\n",
        );
        assert_eq!(config.resolve("db.internal").user.as_deref(), Some("admin"));
        assert_eq!(
            config.resolve("secret.internal").user.as_deref(),
            Some("a"),
            "negationen ska undanta värden"
        );
    }

    /// En `Match`-rad utan kriterier är trasig och ska inte aktivera något.
    /// `host` utan mönster likaså.
    #[test]
    fn a_match_line_without_usable_criteria_never_activates() {
        for line in ["Match\n", "Match host\n"] {
            let config = SSHConfig::parse(&format!("{line}  User skulle-inte-synas\n\nHost *\n  User riktig\n"));
            assert_eq!(config.resolve("x").user.as_deref(), Some("riktig"), "rad: {line:?}");
        }
    }

    /// `Match`-block får aldrig bidra med värdalias till importen. De
    /// beskriver villkor, inte värdar — ett alias därifrån skulle skapa en
    /// post för något som inte är en server.
    #[test]
    fn match_blocks_contribute_no_host_aliases() {
        let config = SSHConfig::parse("Match host produktion\n  User a\n\nHost riktig\n  User b\n");
        assert_eq!(config.host_aliases(), vec!["riktig".to_string()]);
    }

    // MARK: Fält importen tidigare slängde

    /// `ForwardAgent` är ett säkerhetsval, så bara ett uttryckligt `yes`
    /// räknas. Allt annat — `no`, skräp, eller att nyckeln saknas — är nej.
    /// Att gissa fel åt andra hållet skulle slå på agentvidarebefordran åt
    /// någon som inte bett om det.
    #[test]
    fn forward_agent_requires_an_explicit_yes() {
        let config = SSHConfig::parse(
            "Host ja\n  ForwardAgent yes\n\nHost stort\n  ForwardAgent YES\n\n             Host nej\n  ForwardAgent no\n\nHost skrap\n  ForwardAgent kanske\n\n             Host inget\n  User a\n",
        );
        assert!(config.resolve("ja").forward_agent);
        assert!(config.resolve("stort").forward_agent, "nyckelordet är skiftlägesokänsligt");
        assert!(!config.resolve("nej").forward_agent);
        assert!(!config.resolve("skrap").forward_agent, "obegripligt värde är inte ja");
        assert!(!config.resolve("inget").forward_agent, "frånvaro är nej");
    }

    /// `RemoteCommand` motsvarar `Host::startup_command`. Fältet fanns redan,
    /// importen fyllde det bara aldrig i.
    #[test]
    fn remote_command_becomes_the_startup_command() {
        let config = SSHConfig::parse(
            "Host m\n  HostName m.example\n  User a\n  RemoteCommand tmux attach\n  ForwardAgent yes\n",
        );
        let hosts = imported_hosts(&config);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].startup_command.as_deref(), Some("tmux attach"));
        assert!(hosts[0].forward_agent);
    }

    /// `ProxyJump` skrivs `[user@]host[:port]` och kan ange en kedja. Bara
    /// första hoppets värdnamn är intressant — `HostStore::resolve_jump`
    /// avvisar ändå längre kedjor, så en importerad kedja vore en koppling
    /// som inte går att använda.
    #[test]
    fn the_proxy_jump_alias_is_extracted_from_every_written_form() {
        assert_eq!(proxy_jump_alias("bastion").as_deref(), Some("bastion"));
        assert_eq!(proxy_jump_alias("anders@bastion").as_deref(), Some("bastion"));
        assert_eq!(proxy_jump_alias("anders@bastion:2222").as_deref(), Some("bastion"));
        assert_eq!(proxy_jump_alias("bastion,inre").as_deref(), Some("bastion"), "bara första hoppet");
        assert_eq!(proxy_jump_alias("[::1]:22").as_deref(), Some("[::1]"), "IPv6 får inte klippas vid kolon");
        assert_eq!(proxy_jump_alias("none"), None, "OpenSSH:s sätt att stänga av ett ärvt ProxyJump");
        assert_eq!(proxy_jump_alias(""), None);
    }

    /// Hela vägen: en config med ProxyJump ska ge en värd som faktiskt PEKAR
    /// på jump-hosten i databasen. Utan kopplingen misslyckas anslutningen
    /// helt, eftersom målet bara är nåbart genom hoppet.
    ///
    /// Jump-hosten står EFTER den som pekar på den, vilket är hela skälet
    /// till att kopplingen sker i ett andra pass — vid första passet finns
    /// inget id att peka på än.
    #[test]
    fn importing_links_proxy_jump_to_the_actual_jump_host() {
        let dir = temp_config_dir();
        let mut store = crate::host::HostStore::open(dir.join("hosts.json")).unwrap();
        let text = "Host inre\n  HostName 10.0.0.9\n  User a\n  ProxyJump anders@hopp:2222\n\n                    Host hopp\n  HostName hopp.example\n  User anders\n";
        assert_eq!(store.import_ssh_config(text).unwrap(), 2);

        let all = store.all();
        let inre = all.iter().find(|h| h.alias == "inre").expect("inre saknas");
        let hopp = all.iter().find(|h| h.alias == "hopp").expect("hopp saknas");
        assert_eq!(inre.jump_host_id, Some(hopp.id), "ProxyJump ska peka på den importerade jump-hosten");
        assert_eq!(hopp.jump_host_id, None, "jump-hosten själv har inget hopp");
        std::fs::remove_dir_all(dir).ok();
    }

    /// Pekar ProxyJump på något som inte importerades ska värden ändå sparas,
    /// bara utan koppling. Ett halvt resultat är bättre än inget alls.
    #[test]
    fn an_unresolvable_proxy_jump_still_imports_the_host() {
        let dir = temp_config_dir();
        let mut store = crate::host::HostStore::open(dir.join("hosts.json")).unwrap();
        let text = "Host inre\n  HostName 10.0.0.9\n  User a\n  ProxyJump finns-inte\n";
        assert_eq!(store.import_ssh_config(text).unwrap(), 1);
        assert_eq!(store.all()[0].jump_host_id, None);
        std::fs::remove_dir_all(dir).ok();
    }

    /// En värd som anger sig själv som ProxyJump får inte länkas — det vore
    /// ingen kedja, bara en anslutning som aldrig kan lyckas.
    #[test]
    fn a_host_that_names_itself_as_proxy_jump_is_not_linked() {
        let dir = temp_config_dir();
        let mut store = crate::host::HostStore::open(dir.join("hosts.json")).unwrap();
        let text = "Host sig-sjalv\n  HostName x.example\n  User a\n  ProxyJump sig-sjalv\n";
        assert_eq!(store.import_ssh_config(text).unwrap(), 1);
        assert_eq!(store.all()[0].jump_host_id, None);
        std::fs::remove_dir_all(dir).ok();
    }
}
