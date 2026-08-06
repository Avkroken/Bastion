//! SFTP-bläddrare, port av App/SFTPBrowserModel.swift (kärnfunktioner).
//! Kör på en egen bakgrundstråd (egen tokio-runtime, precis som `ssh::run`)
//! och tar emot kommandon via en kanal — en enda SFTP-session återanvänds
//! för hela bläddringen, precis som Swiftsidans `ensureClient()`-cache.
//!
//! chmod/chown motsvarar Swiftsidans SFTPClient.setPermissions/chown.
//! komprimera/packa upp shellar ut till tar/zip via `ssh::run_command`
//! (se `archive.rs`) — SFTP version 3 har ingen egen arkivsemantik,
//! samma mönster som Swiftsidans ArchiveOperations.swift.

use crate::host::Host;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

enum Command {
    List { path: String, reply: async_channel::Sender<Result<Vec<Entry>, String>> },
    Read { path: String, reply: async_channel::Sender<Result<Vec<u8>, String>> },
    Write { path: String, data: Vec<u8>, reply: async_channel::Sender<Result<(), String>> },
    Mkdir { path: String, reply: async_channel::Sender<Result<(), String>> },
    RemoveFile { path: String, reply: async_channel::Sender<Result<(), String>> },
    RemoveDir { path: String, reply: async_channel::Sender<Result<(), String>> },
    Rename { from: String, to: String, reply: async_channel::Sender<Result<(), String>> },
    Chmod { path: String, mode: u32, reply: async_channel::Sender<Result<(), String>> },
    Chown { path: String, uid: u32, gid: u32, reply: async_channel::Sender<Result<(), String>> },
}

#[derive(Clone)]
pub struct SftpHandle {
    tx: async_channel::Sender<Command>,
}

impl SftpHandle {
    pub async fn list(&self, path: String) -> Result<Vec<Entry>, String> {
        let (reply, rx) = async_channel::bounded(1);
        self.send(Command::List { path, reply }, rx).await
    }

    pub async fn read(&self, path: String) -> Result<Vec<u8>, String> {
        let (reply, rx) = async_channel::bounded(1);
        self.send(Command::Read { path, reply }, rx).await
    }

    pub async fn write(&self, path: String, data: Vec<u8>) -> Result<(), String> {
        let (reply, rx) = async_channel::bounded(1);
        self.send(Command::Write { path, data, reply }, rx).await
    }

    pub async fn mkdir(&self, path: String) -> Result<(), String> {
        let (reply, rx) = async_channel::bounded(1);
        self.send(Command::Mkdir { path, reply }, rx).await
    }

    pub async fn remove_file(&self, path: String) -> Result<(), String> {
        let (reply, rx) = async_channel::bounded(1);
        self.send(Command::RemoveFile { path, reply }, rx).await
    }

    pub async fn remove_dir(&self, path: String) -> Result<(), String> {
        let (reply, rx) = async_channel::bounded(1);
        self.send(Command::RemoveDir { path, reply }, rx).await
    }

    pub async fn rename(&self, from: String, to: String) -> Result<(), String> {
        let (reply, rx) = async_channel::bounded(1);
        self.send(Command::Rename { from, to, reply }, rx).await
    }

    /// `mode`: en oktal siffra som ett heltal, t.ex. 0o755 — samma notation
    /// som `chmod` på kommandoraden.
    pub async fn chmod(&self, path: String, mode: u32) -> Result<(), String> {
        let (reply, rx) = async_channel::bounded(1);
        self.send(Command::Chmod { path, mode, reply }, rx).await
    }

    pub async fn chown(&self, path: String, uid: u32, gid: u32) -> Result<(), String> {
        let (reply, rx) = async_channel::bounded(1);
        self.send(Command::Chown { path, uid, gid, reply }, rx).await
    }

    async fn send<T>(&self, cmd: Command, rx: async_channel::Receiver<Result<T, String>>) -> Result<T, String> {
        // Går aldrig i praktiken (bakgrundstråden svarar alltid), men
        // undviker att hänga för evigt om tråden redan dött.
        if self.tx.send(cmd).await.is_err() {
            return Err("SFTP-bakgrundstråden är inte längre igång".to_string());
        }
        rx.recv().await.unwrap_or_else(|_| Err("SFTP-bakgrundstråden svarade aldrig".to_string()))
    }
}

/// Laddar upp en lokal fil ELLER mapp REKURSIVT till `remote_path` —
/// motsvarar App/:s drag & drop-uppladdning, en lucka som var dokumenterad
/// som LinuxApp-specifik (SwiftCrossUIs `Gtk`-paket saknade en färdig
/// `GtkDropTarget`-omslag; gtk4-rs, den nya native Rust-porten, har det
/// direkt). Mappar blir `mkdir` + samma funktion per barn i tur och
/// ordning; filer blir läs+skriv. `Box::pin` krävs eftersom en `async fn`
/// inte kan vara rekursiv rakt av (obestämd storlek vid kompilering).
///
/// `mkdir`-fel ignoreras medvetet (samma SFTP v3-begränsning som redan
/// dokumenterats för mapp-skapande på andra ställen: ingen egen "finns
/// redan"-statuskod förrän v6) — om katalogen redan finns fortsätter
/// uppladdningen ändå in i den; ett EKTA behörighetsfel avslöjas istället
/// av den efterföljande skrivningen, som INTE ignoreras.
pub fn upload_path_recursive<'a>(
    handle: &'a SftpHandle,
    local_path: &'a std::path::Path,
    remote_path: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + 'a>> {
    Box::pin(async move {
        if local_path.is_dir() {
            let _ = handle.mkdir(remote_path.to_string()).await;
            let entries = std::fs::read_dir(local_path).map_err(|e| e.to_string())?;
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let child_remote = if remote_path == "." { name.clone() } else { format!("{remote_path}/{name}") };
                upload_path_recursive(handle, &entry.path(), &child_remote).await?;
            }
            Ok(())
        } else {
            let data = std::fs::read(local_path).map_err(|e| e.to_string())?;
            handle.write(remote_path.to_string(), data).await
        }
    })
}

/// Startar SFTP-anslutningen på en ny bakgrundstråd. Om själva anslutningen
/// misslyckas svarar handtaget med samma fel på varje efterföljande
/// kommando istället för att panika eller hänga tyst.
pub fn spawn(host: Host, password: Option<String>, jump: Option<Host>) -> SftpHandle {
    let (tx, rx) = async_channel::unbounded::<Command>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för SFTP-tråden");
        rt.block_on(async move {
            match connect_sftp(host, password, jump).await {
                Ok(session) => run(session, rx).await,
                Err(e) => {
                    while let Ok(cmd) = rx.recv().await {
                        reply_error(cmd, &e);
                    }
                }
            }
        });
    });
    SftpHandle { tx }
}

async fn connect_sftp(
    host: Host,
    password: Option<String>,
    jump: Option<Host>,
) -> Result<SftpSession, String> {
    let session = crate::ssh::connect(&host, password, None, jump).await?;
    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("kunde inte öppna kanal: {e}"))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| format!("sftp-subsystemet nekades: {e}"))?;
    SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| format!("sftp-handskakning misslyckades: {e}"))
}

async fn run(session: SftpSession, rx: async_channel::Receiver<Command>) {
    while let Ok(cmd) = rx.recv().await {
        match cmd {
            Command::List { path, reply } => {
                let result = list(&session, &path).await;
                let _ = reply.send(result).await;
            }
            Command::Read { path, reply } => {
                let result = session.read(path).await.map_err(|e| e.to_string());
                let _ = reply.send(result).await;
            }
            Command::Write { path, data, reply } => {
                let result = write_file(&session, path, &data).await;
                let _ = reply.send(result).await;
            }
            Command::Mkdir { path, reply } => {
                let result = session.create_dir(path).await.map_err(|e| e.to_string());
                let _ = reply.send(result).await;
            }
            Command::RemoveFile { path, reply } => {
                let result = session.remove_file(path).await.map_err(|e| e.to_string());
                let _ = reply.send(result).await;
            }
            Command::RemoveDir { path, reply } => {
                let result = session.remove_dir(path).await.map_err(|e| e.to_string());
                let _ = reply.send(result).await;
            }
            Command::Rename { from, to, reply } => {
                let result = session.rename(from, to).await.map_err(|e| e.to_string());
                let _ = reply.send(result).await;
            }
            Command::Chmod { path, mode, reply } => {
                let result = chmod(&session, &path, mode).await;
                let _ = reply.send(result).await;
            }
            Command::Chown { path, uid, gid, reply } => {
                let result = chown(&session, &path, uid, gid).await;
                let _ = reply.send(result).await;
            }
        }
    }
}

/// `SftpSession::write` bara öppnar med `OpenFlags::WRITE`, vilket
/// misslyckas med "No such file" om filen inte redan finns — till skillnad
/// från Swiftsidans `SFTPClient.writeFile` (öppnar alltid med skapa-flaggan,
/// eftersom både "spara en ny fil" och "spara en ändrad fil" ska fungera).
async fn write_file(session: &SftpSession, path: String, data: &[u8]) -> Result<(), String> {
    let mut file = session
        .open_with_flags(path, OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE)
        .await
        .map_err(|e| e.to_string())?;
    file.write_all(data).await.map_err(|e| e.to_string())?;
    file.shutdown().await.map_err(|e| e.to_string())
}

/// `mode`: oktala behörighetsbitar (t.ex. `0o755`) — motsvarar Swiftsidans
/// `SFTPClient.setPermissions(mode:)`. Skickar bara `permissions`-fältet
/// (SFTP setstat rör bara de fält som faktiskt sätts, resten lämnas orört).
async fn chmod(session: &SftpSession, path: &str, mode: u32) -> Result<(), String> {
    let mut attrs = russh_sftp::protocol::FileAttributes::empty();
    attrs.permissions = Some(mode);
    session.set_metadata(path, attrs).await.map_err(|e| e.to_string())
}

/// `uid`/`gid`: NUMERISKA ID:n, inte användarnamn — SFTP version 3 känner
/// bara till UID/GID, aldrig namn (samma begränsning som Swiftsidans
/// `SFTPClient.chown`).
async fn chown(session: &SftpSession, path: &str, uid: u32, gid: u32) -> Result<(), String> {
    let mut attrs = russh_sftp::protocol::FileAttributes::empty();
    attrs.uid = Some(uid);
    attrs.gid = Some(gid);
    session.set_metadata(path, attrs).await.map_err(|e| e.to_string())
}

async fn list(session: &SftpSession, path: &str) -> Result<Vec<Entry>, String> {
    let read_dir = session.read_dir(path).await.map_err(|e| e.to_string())?;
    let mut entries: Vec<Entry> = read_dir
        .filter(|e| e.file_name() != "." && e.file_name() != "..")
        .map(|e| Entry { name: e.file_name(), is_dir: e.file_type().is_dir(), size: e.metadata().len() })
        .collect();
    // Mapp-först, sedan alfabetiskt inom varje grupp — samma sortering som
    // Swiftsidans `sortedEntries`.
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(entries)
}

fn reply_error(cmd: Command, message: &str) {
    match cmd {
        Command::List { reply, .. } => {
            let _ = reply.send_blocking(Err(message.to_string()));
        }
        Command::Read { reply, .. } => {
            let _ = reply.send_blocking(Err(message.to_string()));
        }
        Command::Write { reply, .. }
        | Command::Mkdir { reply, .. }
        | Command::RemoveFile { reply, .. }
        | Command::RemoveDir { reply, .. }
        | Command::Rename { reply, .. }
        | Command::Chmod { reply, .. }
        | Command::Chown { reply, .. } => {
            let _ = reply.send_blocking(Err(message.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{Host, HostAuth};

    /// Riktig end-to-end-övning mot localhosts sshd: mkdir/write/read/list/
    /// rename/remove i en engångsmapp under /tmp — skapar och tar bort sin
    /// EGEN testmapp, rör aldrig något annat på testmaskinen.
    #[test]
    #[ignore = "kräver en riktig localhost-sshd + en nyckel förberedd i authorized_keys, se ROADMAP.md"]
    fn full_round_trip_against_a_real_sftp_server() {
        let key_path = std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let dir = format!("/tmp/bastion-sftp-test-{}", uuid::Uuid::new_v4());
        let handle = spawn(host, None, None);

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            handle.mkdir(dir.clone()).await.expect("mkdir misslyckades");

            let file_path = format!("{dir}/hello.txt");
            handle.write(file_path.clone(), b"hej bastion".to_vec()).await.expect("write misslyckades");

            let content = handle.read(file_path.clone()).await.expect("read misslyckades");
            assert_eq!(content, b"hej bastion");

            let entries = handle.list(dir.clone()).await.expect("list misslyckades");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, "hello.txt");
            assert!(!entries[0].is_dir);
            assert_eq!(entries[0].size, 11);

            let renamed_path = format!("{dir}/renamed.txt");
            handle.rename(file_path, renamed_path.clone()).await.expect("rename misslyckades");
            let entries = handle.list(dir.clone()).await.expect("list efter rename misslyckades");
            assert_eq!(entries[0].name, "renamed.txt");

            handle.remove_file(renamed_path).await.expect("remove_file misslyckades");
            handle.remove_dir(dir).await.expect("remove_dir misslyckades");
        });
    }

    /// Verifierar chmod/chown mot en RIKTIG sshd — kollar det faktiska
    /// resultatet via `stat` över den vanliga exec-kanalen (inte bara att
    /// SFTP-anropet returnerade Ok), samma oberoende-verifieringsprincip
    /// som `docker_list_command_parses_real_dockerd_output`.
    #[test]
    #[ignore = "kräver en riktig localhost-sshd + en nyckel förberedd i authorized_keys, se ROADMAP.md"]
    fn chmod_and_chown_apply_on_a_real_sftp_server() {
        let key_path = std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let file_path = format!("/tmp/bastion-chmod-test-{}", uuid::Uuid::new_v4());
        let handle = spawn(host.clone(), None, None);

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            handle.write(file_path.clone(), b"x".to_vec()).await.expect("write misslyckades");
            handle.chmod(file_path.clone(), 0o600).await.expect("chmod misslyckades");
        });

        let stat_rx = crate::ssh::run_command(host.clone(), None, format!("stat -c %a {file_path}"), None);
        let mode = stat_rx.recv_blocking().expect("kanalen stängdes").expect("stat misslyckades");
        assert_eq!(mode.trim(), "600", "chmod applicerades aldrig på riktiga servern");

        let cleanup_rx = crate::ssh::run_command(host, None, format!("rm -f {file_path}"), None);
        cleanup_rx.recv_blocking().ok();
    }

    /// Riktig komprimera→packa-upp-övning: skapar en fil, tar.gz:ar den,
    /// tar bort originalet, packar upp, verifierar att INNEHÅLLET är
    /// bevarat — via `ssh::run_command`, exakt samma väg som Docker-vyn
    /// och SFTP-vyns Komprimera/Packa upp-knappar använder.
    #[test]
    #[ignore = "kräver en riktig localhost-sshd + en nyckel förberedd i authorized_keys, se ROADMAP.md"]
    fn compress_then_extract_round_trips_real_file_content() {
        let key_path = std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let dir = format!("/tmp/bastion-archive-test-{}", uuid::Uuid::new_v4());
        let setup_rx = crate::ssh::run_command(
            host.clone(),
            None,
            format!("mkdir -p {dir} && echo hej-bastion > {dir}/a.txt"),
            None,
        );
        setup_rx.recv_blocking().unwrap().expect("setup misslyckades");

        let compress_cmd = crate::archive::create_tar_gz_command(&["a.txt".to_string()], "out.tar.gz", &dir);
        let compress_rx = crate::ssh::run_command(host.clone(), None, compress_cmd, None);
        compress_rx.recv_blocking().unwrap().expect("komprimering misslyckades");

        let remove_rx = crate::ssh::run_command(host.clone(), None, format!("rm {dir}/a.txt"), None);
        remove_rx.recv_blocking().unwrap().expect("kunde inte ta bort originalet");

        let extract_cmd = crate::archive::extract_tar_gz_command("out.tar.gz", &dir);
        let extract_rx = crate::ssh::run_command(host.clone(), None, extract_cmd, None);
        extract_rx.recv_blocking().unwrap().expect("uppackning misslyckades");

        let read_rx = crate::ssh::run_command(host.clone(), None, format!("cat {dir}/a.txt"), None);
        let content = read_rx.recv_blocking().unwrap().expect("kunde inte läsa uppackad fil");
        assert_eq!(content.trim(), "hej-bastion", "innehållet överlevde inte komprimera→packa-upp");

        let cleanup_rx = crate::ssh::run_command(host, None, format!("rm -rf {dir}"), None);
        cleanup_rx.recv_blocking().ok();
    }

    /// Fristående test-sshd (egen konfig/port, INTE systemtjänsten) — samma
    /// teknik som `port_forward`/`socks_proxy`/`key_deploy`, används här så
    /// `upload_path_recursive` kan verifieras utan manuell
    /// `authorized_keys`-uppsättning (ovanstående tester i den här filen är
    /// alla `#[ignore]`-gatade mot just det kravet).
    struct TestSshd {
        child: std::process::Child,
        port: u16,
        dir: std::path::PathBuf,
    }

    impl TestSshd {
        fn start() -> Option<Self> {
            let dir = std::env::temp_dir().join(format!("bastion-sftp-upload-sshd-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).ok()?;

            let host_key = dir.join("hostkey");
            let status = std::process::Command::new("ssh-keygen")
                .args(["-q", "-N", "", "-t", "ed25519", "-f"])
                .arg(&host_key)
                .status()
                .ok()?;
            if !status.success() {
                return None;
            }
            let client_key = dir.join("client_key");
            let status = std::process::Command::new("ssh-keygen")
                .args(["-q", "-N", "", "-t", "ed25519", "-f"])
                .arg(&client_key)
                .status()
                .ok()?;
            if !status.success() {
                return None;
            }
            let client_pub = std::fs::read_to_string(dir.join("client_key.pub")).ok()?;
            std::fs::write(dir.join("authorized_keys"), client_pub).ok()?;

            let port = crate::test_support::reserve_port()?;
            let config_path = dir.join("sshd_config");
            std::fs::write(
                &config_path,
                format!(
                    "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\n\
                     PubkeyAuthentication yes\nPasswordAuthentication no\nUsePAM no\nStrictModes no\n\
                     Subsystem sftp internal-sftp\nPidFile {}\n",
                    host_key.display(),
                    dir.join("authorized_keys").display(),
                    dir.join("pid").display()
                ),
            )
            .ok()?;

            let mut child = std::process::Command::new("/usr/sbin/sshd")
                .args(["-f"])
                .arg(&config_path)
                .args(["-D", "-e"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok()?;

            if !crate::test_support::wait_until_listening(&mut child, port) {
                let _ = std::fs::remove_dir_all(&dir);
                return None;
            }
            Some(TestSshd { child, port, dir })
        }

        fn client_key_path(&self) -> String {
            self.dir.join("client_key").to_string_lossy().into_owned()
        }
    }

    impl Drop for TestSshd {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Skapar en lokal katalogstruktur (en fil i roten + en underkatalog med
    /// en egen fil) och laddar upp den REKURSIVT mot en riktig, fristående
    /// sshd — bevisar att både mapp-rekursionen och fil-innehållet överlever
    /// hela resan, inte bara ett enskilt plant fall.
    #[test]
    fn upload_path_recursive_uploads_a_nested_directory_tree() {
        let Some(sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let mut host = Host::new("upload-test".into(), "127.0.0.1".into(), whoami_user());
        host.port = sshd.port as i64;
        host.auth = HostAuth::KeyFile(sshd.client_key_path());

        let local_dir = std::env::temp_dir().join(format!("bastion-upload-local-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(local_dir.join("subdir")).unwrap();
        std::fs::write(local_dir.join("root.txt"), b"hej-fran-roten").unwrap();
        std::fs::write(local_dir.join("subdir/child.txt"), b"hej-fran-undermappen").unwrap();

        let remote_dir = format!("/tmp/bastion-upload-remote-{}", uuid::Uuid::new_v4());
        let handle = spawn(host, None, None);

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            upload_path_recursive(&handle, &local_dir, &remote_dir).await.expect("uppladdning misslyckades");

            let root_content = handle.read(format!("{remote_dir}/root.txt")).await.expect("kunde inte läsa root.txt");
            assert_eq!(root_content, b"hej-fran-roten");

            let child_content =
                handle.read(format!("{remote_dir}/subdir/child.txt")).await.expect("kunde inte läsa subdir/child.txt");
            assert_eq!(child_content, b"hej-fran-undermappen");

            let root_entries = handle.list(remote_dir.clone()).await.expect("list misslyckades");
            assert_eq!(root_entries.len(), 2, "väntade root.txt + subdir, fick {root_entries:?}");
        });

        std::fs::remove_dir_all(&local_dir).ok();
    }

    fn whoami_user() -> String {
        std::env::var("USER").unwrap_or_else(|_| "test".into())
    }
}
