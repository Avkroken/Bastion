//! Port av Sources/SSHCore/AppSettings.swift — klientbred (inte per värd)
//! inställning för vilka valfria funktionsknappar som visas. Delar fil och
//! fältnamn med Swift-sidan (`~/.bastion/settings.json`), så en användare
//! som synkar sin hemkatalog mellan Mac och Linux får samma val på båda.
//!
//! `LinuxApp` läser/skriver ALLA sex fälten (för att inte tappa en annan
//! klients inställningar vid en delad fil) men gör just nu bara något med
//! `docker_enabled` — Snippets/Kommandobibliotek/SFTP/portvidarebefordran/
//! SSH-nyckeldistribution har ingen vy att gömma än (se ROADMAP.md).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureToggles {
    #[serde(default = "default_true")]
    pub show_docker: bool,
    #[serde(default = "default_true")]
    pub show_snippets: bool,
    #[serde(default = "default_true")]
    pub show_command_library: bool,
    /// Swifts fältnamn är `showSFTPBrowser` (SFTP versalt, en akronym) —
    /// serdes automatiska `camelCase` hade gett `showSftpBrowser` istället,
    /// därav den explicita `rename` här trots `rename_all` på structen.
    #[serde(rename = "showSFTPBrowser", default = "default_true")]
    pub show_sftp_browser: bool,
    #[serde(default = "default_true")]
    pub show_port_forward: bool,
    #[serde(default = "default_true")]
    pub show_key_deploy: bool,
}

fn default_true() -> bool {
    true
}

impl Default for FeatureToggles {
    fn default() -> Self {
        FeatureToggles {
            show_docker: true,
            show_snippets: true,
            show_command_library: true,
            show_sftp_browser: true,
            show_port_forward: true,
            show_key_deploy: true,
        }
    }
}

pub struct AppSettingsStore {
    path: std::path::PathBuf,
    toggles: FeatureToggles,
}

impl AppSettingsStore {
    pub fn default_path() -> std::path::PathBuf {
        dirs::home_dir()
            .expect("kunde inte hitta hemkatalogen")
            .join(".bastion/settings.json")
    }

    pub fn open(path: std::path::PathBuf) -> Self {
        let toggles = Self::load(&path);
        AppSettingsStore { path, toggles }
    }

    fn load(path: &std::path::Path) -> FeatureToggles {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default()
    }

    pub fn current(&self) -> FeatureToggles {
        self.toggles
    }

    /// Skriver till disk INNAN `current()` uppdateras — om skrivningen
    /// misslyckas ska `current()` fortsätta returnera föregående värde,
    /// annars hade GUI:t tyst visat ett läge som reverterar efter omstart
    /// utan att användaren fått veta att det aldrig sparades.
    pub fn update(&mut self, new_value: FeatureToggles) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let json = serde_json::to_string_pretty(&new_value)?;
        std::fs::write(&self.path, json)?;
        self.toggles = new_value;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_all_true_so_upgrades_dont_lose_buttons() {
        let t = FeatureToggles::default();
        assert!(t.show_docker && t.show_snippets && t.show_command_library);
        assert!(t.show_sftp_browser && t.show_port_forward && t.show_key_deploy);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("bastion-settings-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("settings.json");
        let mut store = AppSettingsStore::open(path.clone());
        let mut toggles = store.current();
        toggles.show_docker = false;
        store.update(toggles).unwrap();

        let reopened = AppSettingsStore::open(path);
        assert!(!reopened.current().show_docker);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn wire_format_matches_a_real_swift_encoding() {
        // Verifierat mot en riktig `swift`-körning (samma struct, JSONEncoder):
        // {"showCommandLibrary":true,"showDocker":true,"showKeyDeploy":true,
        //  "showPortForward":true,"showSFTPBrowser":true,"showSnippets":true}
        let json = serde_json::to_string(&FeatureToggles::default()).unwrap();
        assert!(json.contains("\"showDocker\":true"));
        assert!(json.contains("\"showSFTPBrowser\":true"), "fick: {json}");
        assert!(!json.contains("showSftpBrowser"), "serdes auto-camelCase skulle ha gett fel casing: {json}");
    }

    #[test]
    fn a_missing_field_defaults_to_true_not_a_decode_error() {
        // Motsvarar en äldre settings.json innan ett fält tillkom.
        let partial = r#"{"showDocker":false}"#;
        let t: FeatureToggles = serde_json::from_str(partial).unwrap();
        assert!(!t.show_docker);
        assert!(t.show_snippets);
    }
}
