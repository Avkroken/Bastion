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
