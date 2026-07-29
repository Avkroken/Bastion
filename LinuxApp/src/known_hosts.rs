//! Trust-on-first-use-lagring av värdnycklar. Port av
//! Sources/SSHCore/KnownHosts.swift — samma filformat (`host:port
//! ssh-ed25519 AAAA...`, en rad per värd) så `~/.bastion/known_hosts`
//! kan i princip delas/synkas mellan klienter, även om `LinuxApp` inte
//! deltar i synkprotokollet än (se ROADMAP.md/task #5).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Matchar lagrad nyckel.
    Trusted,
    /// Ny värd — nu inlärd.
    Learned,
    /// Skiljer sig från lagrad nyckel (potentiell MITM eller ombyggd server).
    Changed(String),
}

pub struct KnownHosts {
    path: Option<PathBuf>,
    entries: Mutex<HashMap<String, String>>,
}

impl KnownHosts {
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .expect("kunde inte hitta hemkatalogen")
            .join(".bastion/known_hosts")
    }

    pub fn open(path: Option<PathBuf>) -> Self {
        let entries = Self::load(path.as_deref());
        KnownHosts { path, entries: Mutex::new(entries) }
    }

    fn load(path: Option<&Path>) -> HashMap<String, String> {
        let Some(path) = path else { return HashMap::new() };
        let Ok(text) = std::fs::read_to_string(path) else { return HashMap::new() };
        text.lines()
            .filter_map(|line| line.split_once(' '))
            .map(|(id, key)| (id.to_string(), key.to_string()))
            .collect()
    }

    /// Avgör hur en presenterad nyckel förhåller sig till vad vi sett
    /// tidigare. Lär in nyckeln (och persisterar) om värden är ny.
    pub fn check(&self, host: &str, port: u16, key_string: &str) -> Verdict {
        let id = format!("{host}:{port}");
        let mut entries = self.entries.lock().expect("known_hosts-låset förgiftat");
        if let Some(stored) = entries.get(&id) {
            if stored == key_string {
                Verdict::Trusted
            } else {
                Verdict::Changed(stored.clone())
            }
        } else {
            entries.insert(id.clone(), key_string.to_string());
            self.append(&id, key_string);
            Verdict::Learned
        }
    }

    fn append(&self, id: &str, key_string: &str) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
            }
        }
        let existed = path.exists();
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(format!("{id} {key_string}\n").as_bytes());
        }
        if !existed {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!("bastion-known-hosts-test-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn learns_a_new_host_then_trusts_it() {
        let path = temp_path();
        let kh = KnownHosts::open(Some(path.clone()));
        assert_eq!(kh.check("example.com", 22, "ssh-ed25519 AAAA1"), Verdict::Learned);
        assert_eq!(kh.check("example.com", 22, "ssh-ed25519 AAAA1"), Verdict::Trusted);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn detects_a_changed_key() {
        let path = temp_path();
        let kh = KnownHosts::open(Some(path.clone()));
        kh.check("example.com", 22, "ssh-ed25519 AAAA1");
        let verdict = kh.check("example.com", 22, "ssh-ed25519 AAAA2-ANNAN-NYCKEL");
        assert_eq!(verdict, Verdict::Changed("ssh-ed25519 AAAA1".into()));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn persists_across_reopen() {
        let path = temp_path();
        {
            let kh = KnownHosts::open(Some(path.clone()));
            kh.check("mp100", 22, "ssh-ed25519 AAAAPERSIST");
        }
        let reopened = KnownHosts::open(Some(path.clone()));
        assert_eq!(reopened.check("mp100", 22, "ssh-ed25519 AAAAPERSIST"), Verdict::Trusted);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn different_ports_are_independent_hosts() {
        let path = temp_path();
        let kh = KnownHosts::open(Some(path.clone()));
        assert_eq!(kh.check("h", 22, "keyA"), Verdict::Learned);
        assert_eq!(kh.check("h", 2222, "keyB"), Verdict::Learned);
        std::fs::remove_file(path).ok();
    }
}
