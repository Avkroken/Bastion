//! SSH-anslutning via russh, körs på en egen bakgrundstråd (egen
//! single-thread tokio-runtime) eftersom GTK:s huvudloop är glib, inte tokio.
//! Kommunicerar med UI-tråden via `async_channel` (Send+Sync, kan pollas från
//! både tokio och glibs `spawn_local`).
//!
//! KÄND BEGRÄNSNING (dokumenterad, inte dold): `check_server_key` accepterar
//! just nu ALLA värdnycklar utan verifiering. Sources/SSHCore/KnownHosts.swift
//! + HostKeyValidator.swift gör riktig TOFU-verifiering på Apple-sidan — samma
//! logik måste porteras hit innan detta är produktionsklart. Se ROADMAP.md.
//!
//! KÄND BEGRÄNSNING: bara `HostAuth::KeyFile` (utan lösenfras),
//! `HostAuth::AgentDefault` (ssh-agent) och `HostAuth::AskPassword`
//! (lösenord) stöds. `KeychainKey`/`CertificateFile`/`BitwardenItem` är
//! Apple/Keychain- respektive Bitwarden-specifika och saknar en Linux-
//! motsvarighet ännu.

use crate::host::{Host, HostAuth};
use russh::client::{self, Handle};
use russh::keys::agent::client::AgentClient;
use russh::keys::key::PublicKey;
use russh::keys::load_secret_key;
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

struct ClientHandler;

#[async_trait::async_trait]
impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _server_public_key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true) // TODO(#known-hosts): TOFU-verifiering, se modulnoten ovan
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
            if let Err(e) = run(host, password, cols, rows, input_rx, output_tx.clone()).await {
                let _ = output_tx.send(SshEvent::Error(e)).await;
            }
            let _ = output_tx.send(SshEvent::Closed).await;
        });
    });

    SshSession { input: input_tx, output: output_rx }
}

async fn run(
    host: Host,
    password: Option<String>,
    cols: u32,
    rows: u32,
    input_rx: async_channel::Receiver<Vec<u8>>,
    output_tx: async_channel::Sender<SshEvent>,
) -> Result<(), String> {
    let config = Arc::new(client::Config::default());
    let addr = (host.host_name.as_str(), host.port as u16);
    let mut session: Handle<ClientHandler> = client::connect(config, addr, ClientHandler)
        .await
        .map_err(|e| format!("anslutning misslyckades: {e}"))?;

    authenticate(&mut session, &host, password).await?;

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
mod tests {
    use super::*;
    use crate::host::Host;
    use std::time::Duration;

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
        let mut got_data = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match session.output.recv_blocking() {
                Ok(SshEvent::Data(_)) => {
                    got_data = true;
                    break;
                }
                Ok(SshEvent::Error(e)) => panic!("SSH-fel: {e}"),
                Ok(SshEvent::Closed) => break,
                Ok(SshEvent::Connected) => continue,
                Err(_) => break,
            }
        }
        assert!(got_data, "fick aldrig någon data tillbaka från fjärrskalet");
    }
}
