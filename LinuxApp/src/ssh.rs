//! SSH-anslutning via russh, körs på en egen bakgrundstråd (egen
//! single-thread tokio-runtime) eftersom GTK:s huvudloop är glib, inte tokio.
//! Kommunicerar med UI-tråden via `async_channel` (Send+Sync, kan pollas från
//! både tokio och glibs `spawn_local`).
//!
//! Host-key-verifiering: TOFU via `crate::known_hosts::KnownHosts`, samma
//! princip och filformat som Sources/SSHCore/KnownHosts.swift +
//! HostKeyValidator.swift.
//!
//! KÄND BEGRÄNSNING: bara `HostAuth::KeyFile` (utan lösenfras),
//! `HostAuth::AgentDefault` (ssh-agent) och `HostAuth::AskPassword`
//! (lösenord) stöds. `KeychainKey`/`CertificateFile`/`BitwardenItem` är
//! Apple/Keychain- respektive Bitwarden-specifika och saknar en Linux-
//! motsvarighet ännu.

use crate::host::{Host, HostAuth};
use crate::known_hosts::{KnownHosts, Verdict};
use russh::client::{self, Handle};
use russh::keys::agent::client::AgentClient;
use russh::keys::key::PublicKey;
use russh::keys::{load_secret_key, PublicKeyBase64};
use russh::ChannelMsg;
use std::sync::Arc;

#[derive(Debug)]
pub enum SshEvent {
    Connected,
    Data(Vec<u8>),
    Error(String),
    Closed,
}

pub struct SshSession {
    pub input: async_channel::Sender<Vec<u8>>,
    pub output: async_channel::Receiver<SshEvent>,
}

/// `client::connect`s felväg — måste implementera `From<russh::Error>` för
/// att uppfylla `Handler::Error`s bound, men bär också vårt eget
/// TOFU-avslag med ett förklarande meddelande (istället för `Ok(false)`,
/// som bara ger ett generiskt "UnknownKey").
#[derive(Debug)]
pub(crate) enum ConnectError {
    Russh(russh::Error),
    HostKeyChanged(String),
}

impl From<russh::Error> for ConnectError {
    fn from(e: russh::Error) -> Self {
        ConnectError::Russh(e)
    }
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::Russh(e) => write!(f, "{e}"),
            ConnectError::HostKeyChanged(msg) => write!(f, "{msg}"),
        }
    }
}

pub(crate) struct ClientHandler {
    host: String,
    port: u16,
    known_hosts: Arc<KnownHosts>,
}

#[async_trait::async_trait]
impl client::Handler for ClientHandler {
    type Error = ConnectError;

    async fn check_server_key(&mut self, server_public_key: &PublicKey) -> Result<bool, Self::Error> {
        let key_string = format!("{} {}", server_public_key.name(), server_public_key.public_key_base64());
        match self.known_hosts.check(&self.host, self.port, &key_string) {
            Verdict::Trusted | Verdict::Learned => Ok(true),
            Verdict::Changed(stored) => Err(ConnectError::HostKeyChanged(format!(
                "VÄRDNYCKELN FÖR {}:{} HAR ÄNDRATS — möjlig man-i-mitten-attack eller en \
                 ombyggd server. Lagrad: \"{stored}\" Ny: \"{key_string}\". Om ändringen är \
                 väntad (t.ex. ominstallerad server), ta bort motsvarande rad i \
                 ~/.bastion/known_hosts manuellt.",
                self.host, self.port
            ))),
        }
    }
}

/// Startar SSH-anslutningen på en ny bakgrundstråd och returnerar kanalerna
/// direkt — anropas från GTK-huvudtråden, blockerar inte den.
pub fn spawn_shell(host: Host, password: Option<String>, cols: u32, rows: u32) -> SshSession {
    let (input_tx, input_rx) = async_channel::unbounded::<Vec<u8>>();
    let (output_tx, output_rx) = async_channel::unbounded::<SshEvent>();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för SSH-tråden");
        rt.block_on(async move {
            if let Err(e) = run(host, password, cols, rows, input_rx, output_tx.clone(), None).await {
                let _ = output_tx.send(SshEvent::Error(e)).await;
            }
            let _ = output_tx.send(SshEvent::Closed).await;
        });
    });

    SshSession { input: input_tx, output: output_rx }
}

/// Ansluter och autentiserar — delad av den interaktiva shell-sessionen
/// (`run`) och engångskommandon (`run_command_once`, t.ex. Docker-anrop).
pub(crate) async fn connect(
    host: &Host,
    password: Option<String>,
    known_hosts_path_override: Option<std::path::PathBuf>,
) -> Result<Handle<ClientHandler>, String> {
    let known_hosts = Arc::new(KnownHosts::open(Some(
        known_hosts_path_override.unwrap_or_else(KnownHosts::default_path),
    )));
    let config = Arc::new(client::Config::default());
    let addr = (host.host_name.as_str(), host.port as u16);
    let handler = ClientHandler { host: host.host_name.clone(), port: host.port as u16, known_hosts };
    let mut session: Handle<ClientHandler> = client::connect(config, addr, handler)
        .await
        .map_err(|e| format!("anslutning misslyckades: {e}"))?;
    authenticate(&mut session, host, password).await?;
    Ok(session)
}

/// Kör ETT kommando över en fristående anslutning (ingen pty, ingen
/// interaktiv shell) och returnerar stdout+stderr som text. Används för
/// engångsanrop (Docker list/start/stopp/loggar) — en ny anslutning per
/// anrop är enklare och korrekt, om än inte det mest effektiva; se
/// ROADMAP.md om det senare visar sig behöva en delad uppkopplad session.
pub fn run_command(host: Host, password: Option<String>, command: String) -> async_channel::Receiver<Result<String, String>> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för kommandotråden");
        let result = rt.block_on(run_command_once(host, password, command, None));
        let _ = tx.send_blocking(result);
    });
    rx
}

async fn run_command_once(
    host: Host,
    password: Option<String>,
    command: String,
    known_hosts_path_override: Option<std::path::PathBuf>,
) -> Result<String, String> {
    let session = connect(&host, password, known_hosts_path_override).await?;
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("kunde inte öppna kanal: {e}"))?;
    channel
        .exec(true, command.as_bytes())
        .await
        .map_err(|e| format!("kommandot kunde inte köras: {e}"))?;

    let mut output = Vec::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                output.extend_from_slice(&data);
            }
            ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    String::from_utf8(output).map_err(|e| format!("ogiltig UTF-8 i kommandots utdata: {e}"))
}

async fn run(
    host: Host,
    password: Option<String>,
    cols: u32,
    rows: u32,
    input_rx: async_channel::Receiver<Vec<u8>>,
    output_tx: async_channel::Sender<SshEvent>,
    known_hosts_path_override: Option<std::path::PathBuf>,
) -> Result<(), String> {
    let session = connect(&host, password, known_hosts_path_override).await?;

    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("kunde inte öppna kanal: {e}"))?;
    channel
        .request_pty(false, "xterm-256color", cols, rows, 0, 0, &[])
        .await
        .map_err(|e| format!("pty-begäran nekad: {e}"))?;
    channel
        .request_shell(true)
        .await
        .map_err(|e| format!("shell-begäran nekad: {e}"))?;

    if let Some(cmd) = &host.startup_command {
        if !cmd.is_empty() {
            channel
                .data(format!("{cmd}\n").as_bytes())
                .await
                .map_err(|e| format!("kunde inte skicka startkommando: {e}"))?;
        }
    }

    let _ = output_tx.send(SshEvent::Connected).await;

    loop {
        tokio::select! {
            incoming = input_rx.recv() => {
                match incoming {
                    Ok(bytes) => {
                        if channel.data(&bytes[..]).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break, // UI-sidan stängde input-kanalen
                }
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        if output_tx.send(SshEvent::Data(data.to_vec())).await.is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::ExitStatus { .. }) | None => break,
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

async fn authenticate(
    session: &mut Handle<ClientHandler>,
    host: &Host,
    password: Option<String>,
) -> Result<(), String> {
    let ok = match &host.auth {
        HostAuth::KeyFile(path) => {
            let key = load_secret_key(path, None)
                .map_err(|e| format!("kunde inte läsa nyckelfilen {path}: {e} (lösenfraser stöds inte än)"))?;
            session
                .authenticate_publickey(&host.user, Arc::new(key))
                .await
                .map_err(|e| format!("publik nyckel-autentisering misslyckades: {e}"))?
        }
        HostAuth::AgentDefault => {
            let mut agent = AgentClient::connect_env()
                .await
                .map_err(|e| format!("kunde inte ansluta till ssh-agent: {e}"))?;
            let identities = agent
                .request_identities()
                .await
                .map_err(|e| format!("kunde inte hämta identiteter från ssh-agent: {e}"))?;
            let Some(key) = identities.into_iter().next() else {
                return Err("ssh-agent har inga laddade identiteter".into());
            };
            let (_agent, result) = session.authenticate_future(&host.user, key, agent).await;
            result.map_err(|e| format!("agent-autentisering misslyckades: {e}"))?
        }
        HostAuth::AskPassword => {
            let pass = password.ok_or("lösenord krävs men saknades")?;
            session
                .authenticate_password(&host.user, pass)
                .await
                .map_err(|e| format!("lösenordsautentisering misslyckades: {e}"))?
        }
        other => {
            return Err(format!(
                "autentiseringstypen {other:?} stöds inte på Linux ännu"
            ))
        }
    };
    if !ok {
        return Err("servern avvisade autentiseringen".into());
    }
    Ok(())
}

#[cfg(test)]
fn spawn_shell_with_known_hosts(
    host: Host,
    password: Option<String>,
    cols: u32,
    rows: u32,
    known_hosts_path: std::path::PathBuf,
) -> SshSession {
    let (input_tx, input_rx) = async_channel::unbounded::<Vec<u8>>();
    let (output_tx, output_rx) = async_channel::unbounded::<SshEvent>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            if let Err(e) = run(host, password, cols, rows, input_rx, output_tx.clone(), Some(known_hosts_path)).await
            {
                let _ = output_tx.send(SshEvent::Error(e)).await;
            }
            let _ = output_tx.send(SshEvent::Closed).await;
        });
    });
    SshSession { input: input_tx, output: output_rx }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Host;
    use std::time::Duration;

    fn drain_until_data_error_or_closed(session: &SshSession, timeout: Duration) -> Result<(), String> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            match session.output.recv_blocking() {
                Ok(SshEvent::Data(_)) => return Ok(()),
                Ok(SshEvent::Error(e)) => return Err(e),
                Ok(SshEvent::Closed) => return Err("stängdes utan data eller fel".into()),
                Ok(SshEvent::Connected) => continue,
                Err(_) => return Err("output-kanalen stängdes oväntat".into()),
            }
        }
        Err("timeout".into())
    }

    /// Riktig end-to-end-anslutning mot localhosts sshd (samma tjänst som
    /// `systemctl status ssh` visar aktiv). Kräver en nyckel som redan är
    /// tillagd i `~/.ssh/authorized_keys` — sätts upp/rivs av testskriptet
    /// som körde detta manuellt, inte av testet självt (ingen automatisk
    /// modifiering av användarens authorized_keys från testsviten).
    #[test]
    #[ignore = "kräver en riktig localhost-sshd + en nyckel förberedd i authorized_keys, se ROADMAP.md"]
    fn connects_to_real_localhost_sshd_and_gets_a_shell_prompt() {
        let key_path = std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let session = spawn_shell(host, None, 80, 24);
        assert!(
            drain_until_data_error_or_closed(&session, Duration::from_secs(10)).is_ok(),
            "fick aldrig någon data tillbaka från fjärrskalet"
        );
    }

    /// Samma riktiga sshd, men denna gång med en förorenad known_hosts-fil
    /// (en falsk nyckel förinlagd för 127.0.0.1:22) — verifierar att TOFU
    /// faktiskt AVVISAR anslutningen istället för att bara logga en varning.
    #[test]
    #[ignore = "kräver en riktig localhost-sshd + en nyckel förberedd i authorized_keys, se ROADMAP.md"]
    fn rejects_connection_when_host_key_has_changed() {
        let key_path = std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let known_hosts_path =
            std::env::temp_dir().join(format!("bastion-tofu-test-{}.known_hosts", uuid::Uuid::new_v4()));
        std::fs::write(&known_hosts_path, "127.0.0.1:22 ssh-ed25519 FALSKT-INTE-DEN-RIKTIGA-NYCKELN\n").unwrap();

        let session = spawn_shell_with_known_hosts(host, None, 80, 24, known_hosts_path.clone());
        let result = drain_until_data_error_or_closed(&session, Duration::from_secs(10));
        std::fs::remove_file(&known_hosts_path).ok();

        match result {
            Err(msg) => assert!(
                msg.contains("HAR ÄNDRATS"),
                "väntade ett host-key-avslag, fick: {msg}"
            ),
            Ok(()) => panic!("anslutningen borde ha avvisats p.g.a. ändrad värdnyckel, men lyckades"),
        }
    }

    /// Verifierar `run_command` (engångs-exec, ingen pty) mot en riktig
    /// sshd — LÄSANDE kommando bara (`docker ps`), rör ALDRIG start/stopp
    /// på riktiga containrar som kan köra på testmaskinen.
    #[test]
    #[ignore = "kräver en riktig localhost-sshd + en nyckel förberedd i authorized_keys, se ROADMAP.md"]
    fn run_command_executes_a_real_readonly_command_over_ssh() {
        let key_path = std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let rx = run_command(host, None, "echo bastion-run-command-ok".to_string());
        let result = rx.recv_blocking().expect("kanalen stängdes utan svar");
        assert_eq!(result.unwrap().trim(), "bastion-run-command-ok");
    }

    /// Docker-vyns list-kommando mot en riktig `dockerd` med riktiga
    /// containrar — LÄSANDE (`docker ps`) bara, rör aldrig start/stopp/
    /// omstart av testmaskinens faktiska containrar.
    #[test]
    #[ignore = "kräver riktig localhost-sshd + docker + en nyckel i authorized_keys, se ROADMAP.md"]
    fn docker_list_command_parses_real_dockerd_output() {
        let key_path = std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let rx = run_command(host, None, crate::docker::list_command(true));
        let output = rx.recv_blocking().expect("kanalen stängdes utan svar").expect("docker ps misslyckades");
        let containers = crate::docker::parse_list(&output);
        assert!(!containers.is_empty(), "väntade minst en container på testmaskinen, fick ingen");
    }

    /// Verifierar att skriva `exit` i den interaktiva shellen faktiskt
    /// stänger SSH-sessionen (får `SshEvent::Closed`) — det uttryckliga
    /// kravet "exit måste avsluta sessionen". `main.rs::start_session`
    /// reagerar på just denna händelse genom att stänga fliken.
    #[test]
    #[ignore = "kräver en riktig localhost-sshd + en nyckel förberedd i authorized_keys, se ROADMAP.md"]
    fn typing_exit_in_the_shell_closes_the_session() {
        let key_path = std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let session = spawn_shell(host, None, 80, 24);
        // Vänta in första skalpromptens data innan vi skriver något, annars
        // kan "exit\n" hamna innan skalet ens är redo att läsa stdin.
        drain_until_data_error_or_closed(&session, Duration::from_secs(10))
            .expect("fick aldrig en initial prompt från skalet");

        session.input.send_blocking(b"exit\n".to_vec()).expect("kunde inte skicka exit till skalet");

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut closed = false;
        while std::time::Instant::now() < deadline {
            match session.output.recv_blocking() {
                Ok(SshEvent::Closed) => {
                    closed = true;
                    break;
                }
                Ok(SshEvent::Error(e)) => panic!("SSH-fel istället för en ren stängning: {e}"),
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        assert!(closed, "sessionen stängdes aldrig efter att exit skrevs i skalet");
    }
}
