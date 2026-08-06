//! Lokal och fjärr-portvidarebefordran (`ssh -L`/`-R`), motsvarigheten till
//! `Sources/SSHCore/PortForward.swift`s `openLocalPortForward`/
//! `openRemotePortForward`. Lokal: en lokal TCP-lyssnare bryggar varje
//! ansluten klient till en egen `direct-tcpip`-SSH-kanal. Fjärr: servern
//! ombeds lyssna åt oss (`tcpip_forward`), och varje inkommen
//! `forwarded-tcpip`-kanal (dirigerad av `ClientHandler::
//! server_channel_open_forwarded_tcpip`, se `ssh.rs`) bryggas mot en ny
//! lokal TCP-anslutning.
//!
//! Dynamisk (`-D`, SOCKS5) vidarebefordran ligger i `socks_proxy.rs` och
//! nås härifrån via `ActiveForward::Dynamic`.

use crate::host::Host;
use russh::client::Handle;
use russh::ChannelStream;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use crate::ssh::{ClientHandler, RemoteForwards};

/// Handtag till en pågående vidarebefordran. `actual_bind_port` avslöjar den
/// verkliga porten när `bind_port: 0` (OS-tilldelad) begärdes — samma mönster
/// som `SSHSession.LocalPortForward.actualBindPort` i Swift.
pub struct LocalPortForward {
    pub actual_bind_port: u16,
    stop_tx: async_channel::Sender<()>,
}

impl LocalPortForward {
    /// Stänger lyssnaren — redan öppna vidarebefordrade anslutningar får
    /// avsluta sina egna kopieringsloopar naturligt (deras
    /// `copy_bidirectional` ser bara att endera sidan stängs).
    pub fn stop(&self) {
        let _ = self.stop_tx.try_send(());
    }
}

/// Startar en lokal vidarebefordran på en egen bakgrundstråd (samma mönster
/// som `ssh::spawn_shell`/`ssh::run_command` — GTK:s huvudloop är glib, inte
/// tokio). Returnerar handtaget via kanalen så länge anslutning+bind lyckas;
/// annars ett felmeddelande.
pub fn spawn_local_forward(
    host: Host,
    password: Option<String>,
    bind_host: String,
    bind_port: u16,
    target_host: String,
    target_port: u16,
    jump: Option<Host>,
) -> async_channel::Receiver<Result<LocalPortForward, String>> {
    let (result_tx, result_rx) = async_channel::bounded(1);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för port-forward-tråden");
        rt.block_on(async move {
            let session = match crate::ssh::connect(&host, password, None, jump).await {
                Ok(s) => Arc::new(s),
                Err(e) => {
                    let _ = result_tx.send(Err(e)).await;
                    return;
                }
            };

            let listener = match TcpListener::bind((bind_host.as_str(), bind_port)).await {
                Ok(l) => l,
                Err(e) => {
                    let _ = result_tx.send(Err(format!("kunde inte binda {bind_host}:{bind_port}: {e}"))).await;
                    return;
                }
            };
            let actual_bind_port = match listener.local_addr() {
                Ok(addr) => addr.port(),
                Err(e) => {
                    let _ = result_tx.send(Err(format!("kunde inte läsa bunden port: {e}"))).await;
                    return;
                }
            };

            let (stop_tx, stop_rx) = async_channel::bounded::<()>(1);
            let _ = result_tx.send(Ok(LocalPortForward { actual_bind_port, stop_tx })).await;

            loop {
                tokio::select! {
                    _ = stop_rx.recv() => break,
                    accepted = listener.accept() => {
                        let (stream, _peer) = match accepted {
                            Ok(pair) => pair,
                            Err(_) => continue,
                        };
                        let session = session.clone();
                        let target_host = target_host.clone();
                        tokio::spawn(async move {
                            let _ = bridge_one_connection(&session, stream, &target_host, target_port).await;
                        });
                    }
                }
            }
        });
    });

    result_rx
}

/// Öppnar EN `direct-tcpip`-kanal och kopierar bytes bidirektionellt tills
/// endera sidan stänger — motsvarar `GlueHandler.swift`/`DirectTCPIPWrapperHandler`
/// i SSHCore, fast via `tokio::io::copy_bidirectional` istället för en egen
/// NIO-handler.
async fn bridge_one_connection(
    session: &Arc<Handle<ClientHandler>>,
    mut local: TcpStream,
    target_host: &str,
    target_port: u16,
) -> Result<(), String> {
    let local_addr = local
        .local_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let local_port = local.local_addr().map(|a| a.port() as u32).unwrap_or(0);

    let channel = session
        .channel_open_direct_tcpip(target_host, target_port as u32, local_addr, local_port)
        .await
        .map_err(|e| format!("kunde inte öppna direct-tcpip-kanal mot {target_host}:{target_port}: {e}"))?;

    let mut remote: ChannelStream<russh::client::Msg> = channel.into_stream();
    let result = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
    let _ = remote.shutdown().await;
    let _ = local.shutdown().await;
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Handtag till en pågående fjärr-vidarebefordran (`ssh -R`). Motsvarar
/// `SSHSession.RemotePortForward` i Swift — `actual_bind_port` avslöjar den
/// port SERVERN faktiskt band när `bind_port: 0` begärdes.
pub struct RemotePortForward {
    pub actual_bind_port: u16,
    stop_tx: async_channel::Sender<()>,
}

impl RemotePortForward {
    /// Signalerar bakgrundstråden att avbeställa vidarebefordran
    /// (`cancel-tcpip-forward`), städa routningskartan och sedan stänga hela
    /// SSH-anslutningen — samma `try_send`-"best effort"-mönster som
    /// `LocalPortForward.stop()`.
    pub fn stop(&self) {
        let _ = self.stop_tx.try_send(());
    }
}

/// Startar en fjärr-vidarebefordran på en egen bakgrundstråd. Sessionen
/// (och dess `RemoteForwards`-karta) hålls vid liv av `RemotePortForward`
/// tills `stop()` anropas — det finns ingen egen accept-loop att avsluta
/// (servern äger lyssnaren), bara SSH-anslutningen som måste hållas öppen.
pub fn spawn_remote_forward(
    host: Host,
    password: Option<String>,
    bind_host: String,
    bind_port: u16,
    target_host: String,
    target_port: u16,
    jump: Option<Host>,
) -> async_channel::Receiver<Result<RemotePortForward, String>> {
    let (result_tx, result_rx) = async_channel::bounded(1);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för fjärr-port-forward-tråden");
        rt.block_on(async move {
            let remote_forwards = RemoteForwards::default();
            let session = match crate::ssh::connect_with_forwards(&host, password, None, remote_forwards.clone(), jump).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = result_tx.send(Err(e)).await;
                    return;
                }
            };

            let actual_bind_port = match session.tcpip_forward(bind_host.clone(), bind_port as u32).await {
                Ok(port) => {
                    // Servern svarar 0 om den band exakt den begärda porten
                    // (bara `bind_port: 0` ger ett äkta OS-tilldelat värde
                    // tillbaka) — samma tolkning som SSHCores `boundPort ??
                    // bindPort`.
                    if port == 0 { bind_port as u32 } else { port }
                }
                Err(e) => {
                    let _ = result_tx
                        .send(Err(format!("servern nekade fjärr-portvidarebefordran (AllowTcpForwarding no?): {e}")))
                        .await;
                    return;
                }
            };
            remote_forwards
                .lock()
                .expect("remote_forwards-låset korrupt")
                .insert(actual_bind_port, (target_host, target_port));

            let (stop_tx, stop_rx) = async_channel::bounded::<()>(1);
            let _ = result_tx.send(Ok(RemotePortForward { actual_bind_port: actual_bind_port as u16, stop_tx })).await;

            // Håller bakgrundstråden (och därmed SSH-anslutningen, vars
            // russh-interna mottagartask kör på DEN HÄR tokio-runtimen) vid
            // liv tills `stop()` signalerar — utan detta skulle tråden
            // (och därmed anslutningen) avslutas direkt efter att svaret
            // skickats.
            let _ = stop_rx.recv().await;
            remote_forwards.lock().expect("remote_forwards-låset korrupt").remove(&actual_bind_port);
            let _ = session.cancel_tcpip_forward(bind_host, actual_bind_port).await;
        });
    });

    result_rx
}

/// Rymmer endera vidarebefordranstypen bakom ett gemensamt handtag, så GTK-
/// vyn (`open_port_forward_view`, `main.rs`) kan hålla EN
/// `Rc<RefCell<Option<ActiveForward>>>` oavsett vilken riktning användaren
/// valde, istället för två parallella fält.
pub enum ActiveForward {
    Local(LocalPortForward),
    Remote(RemotePortForward),
    Dynamic(crate::socks_proxy::DynamicPortForward),
}

impl ActiveForward {
    pub fn actual_bind_port(&self) -> u16 {
        match self {
            ActiveForward::Local(f) => f.actual_bind_port,
            ActiveForward::Remote(f) => f.actual_bind_port,
            ActiveForward::Dynamic(f) => f.actual_bind_port,
        }
    }

    pub fn stop(&self) {
        match self {
            ActiveForward::Local(f) => f.stop(),
            ActiveForward::Remote(f) => f.stop(),
            ActiveForward::Dynamic(f) => f.stop(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{Host, HostAuth};
    use std::io::Write;
    use std::process::{Child, Command};

    /// Startar en FRISTÅENDE, minimal sshd-instans på en slumpad hög port
    /// med en egen konfigfil (läser INTE `/etc/ssh/sshd_config`) — samma
    /// teknik som `bastion-cli -J`-verifieringen (se ROADMAP.md). Träffas
    /// alltså inte av systemets `DenyUsers`-restriktion, som bara gäller
    /// den RIKTIGA systemtjänsten på port 22.
    struct TestSshd {
        child: Child,
        port: u16,
        dir: std::path::PathBuf,
    }

    impl TestSshd {
        fn start() -> Option<Self> {
            let dir = std::env::temp_dir().join(format!("bastion-pf-sshd-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).ok()?;

            let host_key = dir.join("hostkey");
            let status = Command::new("ssh-keygen")
                .args(["-q", "-N", "", "-t", "ed25519", "-f"])
                .arg(&host_key)
                .status()
                .ok()?;
            if !status.success() {
                return None;
            }

            let client_key = dir.join("client_key");
            let status = Command::new("ssh-keygen")
                .args(["-q", "-N", "", "-t", "ed25519", "-f"])
                .arg(&client_key)
                .status()
                .ok()?;
            if !status.success() {
                return None;
            }
            let client_pub = std::fs::read_to_string(dir.join("client_key.pub")).ok()?;
            let authorized_keys = dir.join("authorized_keys");
            std::fs::write(&authorized_keys, client_pub).ok()?;

            // sshd kan inte binda "0" (OS-tilldelad), så en riktig ledig port
            // måste hittas i förväg — och reserveras, annars kan ett annat
            // test i samma process välja samma port ur samma glapp.
            let port = crate::test_support::reserve_port()?;

            let config_path = dir.join("sshd_config");
            std::fs::write(
                &config_path,
                format!(
                    "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\n\
                     PubkeyAuthentication yes\nPasswordAuthentication no\nUsePAM no\nStrictModes no\n\
                     AllowTcpForwarding yes\nPidFile {}\n",
                    host_key.display(),
                    authorized_keys.display(),
                    dir.join("pid").display()
                ),
            )
            .ok()?;

            let mut child = Command::new("/usr/sbin/sshd")
                .args(["-f"])
                .arg(&config_path)
                .args(["-D", "-e"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok()?;

            // Vänta tills VÅR sshd faktiskt lyssnar, inte en gissad fast sleep
            // och inte någon annans lyssnare på samma port.
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

    /// Minimal TCP-ekoserver att vidarebefordra MOT — bevisar att den
    /// vidarebefordrade anslutningen verkligen når ett separat, oberoende
    /// mål genom SSH-tunneln, inte bara en kortsluten loopback-gissning.
    fn spawn_echo_server() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                use std::io::Read;
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if stream.write_all(&buf[..n]).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });
        port
    }

    #[tokio::test]
    async fn local_forward_reaches_a_real_separate_echo_server_through_real_sshd() {
        let Some(sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let echo_port = spawn_echo_server();

        let mut host = Host::new("pf-test".into(), "127.0.0.1".into(), whoami_user());
        host.port = sshd.port as i64;
        host.auth = HostAuth::KeyFile(sshd.client_key_path());

        let rx = spawn_local_forward(
            host,
            None,
            "127.0.0.1".into(),
            0,
            "127.0.0.1".into(),
            echo_port,
            None,
        );
        let forward = rx.recv().await.expect("kanalen stängdes utan svar").expect("forward misslyckades starta");
        assert_ne!(forward.actual_bind_port, 0, "OS-tilldelad port ska vara känd efter start");

        // Genuin klient mot den LOKALA vidarebefordrade porten — inte mot
        // sshd:n eller ekoservern direkt.
        let mut client = TcpStream::connect(("127.0.0.1", forward.actual_bind_port))
            .await
            .expect("kunde inte ansluta till den vidarebefordrade lokala porten");
        client.write_all(b"hej-genom-tunneln").await.unwrap();

        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 64];
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), client.read(&mut buf))
            .await
            .expect("timeout väntade på eko")
            .expect("läsfel");
        assert_eq!(&buf[..n], b"hej-genom-tunneln");

        forward.stop();
    }

    #[tokio::test]
    async fn remote_forward_reaches_a_real_separate_echo_server_through_real_sshd() {
        let Some(sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let echo_port = spawn_echo_server();

        let mut host = Host::new("pf-test-remote".into(), "127.0.0.1".into(), whoami_user());
        host.port = sshd.port as i64;
        host.auth = HostAuth::KeyFile(sshd.client_key_path());

        let rx = spawn_remote_forward(
            host,
            None,
            "127.0.0.1".into(),
            0,
            "127.0.0.1".into(),
            echo_port,
            None,
        );
        let forward = rx.recv().await.expect("kanalen stängdes utan svar").expect("forward misslyckades starta");
        assert_ne!(forward.actual_bind_port, 0, "OS-tilldelad serverport ska vara känd efter start");

        // Genuin klient mot den port SERVERN band åt oss (inte en lokal port
        // vi själva öppnade) — bevisar att sshd faktiskt vidarebefordrar
        // inkommande anslutningar till oss som `forwarded-tcpip`, som vi i
        // sin tur bryggar mot den fristående ekoservern.
        let mut client = TcpStream::connect(("127.0.0.1", forward.actual_bind_port))
            .await
            .expect("kunde inte ansluta till sshd:ns vidarebefordrade port");
        client.write_all(b"hej-genom-fjarrtunneln").await.unwrap();

        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 64];
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), client.read(&mut buf))
            .await
            .expect("timeout väntade på eko")
            .expect("läsfel");
        assert_eq!(&buf[..n], b"hej-genom-fjarrtunneln");

        forward.stop();
    }

    fn whoami_user() -> String {
        std::env::var("USER").unwrap_or_else(|_| "test".into())
    }
}
