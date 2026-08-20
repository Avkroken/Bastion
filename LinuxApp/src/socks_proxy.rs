//! Dynamisk portvidarebefordran (`ssh -D`), motsvarigheten till
//! `Sources/SSHCore/SOCKSProxy.swift`. En lokal SOCKS5-proxy (RFC 1928) —
//! varje ansluten klient (webbläsare, `curl --socks5`, …) förhandlar SOCKS5,
//! och målet den begär VÄLJS AV KLIENTEN PER ANSLUTNING (till skillnad från
//! `-L`/`-R`s fasta mål) — det är det som gör den "dynamisk".
//!
//! Enklare än NIO-versionen: tokio-strömmar läser sekventiellt utan att
//! behöva en egen pipeline-handler/buffertackumulator — `read_exact` blockerar
//! (asynkront) tills hela ramverket finns, ingen manuell fragment-hantering.

use crate::ssh::ClientHandler;
use russh::client::Handle;
use russh::ChannelStream;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::host::Host;

/// Handtag till en pågående dynamisk vidarebefordran. Samma mönster som
/// `LocalPortForward`.
pub struct DynamicPortForward {
    pub actual_bind_port: u16,
    stop_tx: async_channel::Sender<()>,
}

impl DynamicPortForward {
    pub fn stop(&self) {
        let _ = self.stop_tx.try_send(());
    }
}

/// Startar en lokal SOCKS5-proxy på en egen bakgrundstråd — samma mönster
/// som `port_forward::spawn_local_forward`.
pub fn spawn_dynamic_forward(
    host: Host,
    password: Option<String>,
    bind_host: String,
    bind_port: u16,
    jump: Option<Host>,
) -> async_channel::Receiver<Result<DynamicPortForward, String>> {
    let (result_tx, result_rx) = async_channel::bounded(1);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för SOCKS-tråden");
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
            let _ = result_tx.send(Ok(DynamicPortForward { actual_bind_port, stop_tx })).await;

            loop {
                tokio::select! {
                    _ = stop_rx.recv() => break,
                    accepted = listener.accept() => {
                        let (stream, _peer) = match accepted {
                            Ok(pair) => pair,
                            Err(_) => continue,
                        };
                        let session = session.clone();
                        tokio::spawn(async move {
                            let _ = handle_socks_connection(&session, stream).await;
                        });
                    }
                }
            }
        });
    });

    result_rx
}

/// Motsvarande fel som `SOCKSError` i Swift — bara till för intern
/// felrapportering, avvisas alltid tyst (anslutningen stängs) mot klienten,
/// precis som `onError` i `SOCKSHandshakeHandler` gör, så själva orsaken
/// spelar ingen roll för anroparen (till skillnad från Swift-sidan, som
/// beskriver felet i ett användarsynligt sammanhang).
#[derive(Debug)]
enum SocksError {
    UnsupportedVersion,
    NoAcceptableAuthMethod,
    UnsupportedCommand,
    UnsupportedAddressType,
    Io,
}

impl From<std::io::Error> for SocksError {
    fn from(_: std::io::Error) -> Self {
        SocksError::Io
    }
}

/// Genomför HELA SOCKS5-handskakningen (RFC 1928, bara CONNECT/0x01 stöds)
/// på en nyss accepterad anslutning, öppnar en `direct-tcpip`-kanal mot det
/// begärda målet och bryggar sedan bidirektionellt — motsvarar
/// `SOCKSHandshakeHandler` + `completeSOCKSConnect` tillsammans, fast utan
/// pipeline-mellansteg eftersom tokio-strömmar redan är sekventiella.
async fn handle_socks_connection(session: &Arc<Handle<ClientHandler>>, mut stream: TcpStream) -> Result<(), String> {
    match negotiate(&mut stream).await {
        Ok((target_host, target_port)) => {
            let local_addr = stream.local_addr().map(|a| a.ip().to_string()).unwrap_or_else(|_| "127.0.0.1".to_string());
            let local_port = stream.local_addr().map(|a| a.port() as u32).unwrap_or(0);
            match session.channel_open_direct_tcpip(&target_host, target_port as u32, local_addr, local_port).await {
                Ok(channel) => {
                    // REP 0x00 (lyckades), ATYP 0x01 (IPv4), BND.ADDR/PORT
                    // 0.0.0.0:0 — vi binder ingen egen socket, bara tunnlar,
                    // så en meningsfull bind-adress finns inte. Samma
                    // platshållare som Swift-sidan.
                    stream.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await.map_err(|e| e.to_string())?;
                    let mut remote: ChannelStream<russh::client::Msg> = channel.into_stream();
                    let result = tokio::io::copy_bidirectional(&mut stream, &mut remote).await;
                    let _ = remote.shutdown().await;
                    let _ = stream.shutdown().await;
                    result.map(|_| ()).map_err(|e| e.to_string())
                }
                Err(e) => {
                    // REP 0x01 (generellt fel) — täcker alla createChannel-fel,
                    // samma "en gemensam felkod räcker"-avvägning som Swift.
                    let _ = stream.write_all(&[5, 1, 0, 1, 0, 0, 0, 0, 0, 0]).await;
                    Err(format!("kunde inte öppna direct-tcpip-kanal mot {target_host}:{target_port}: {e}"))
                }
            }
        }
        Err(_) => {
            // Ogiltig/oväntad SOCKS-handskakning — stäng utan svar, precis
            // som `onError` i Swift-versionen (`inboundChannel.close`).
            Err("ogiltig SOCKS5-handskakning".to_string())
        }
    }
}

/// SOCKS5-greeting + CONNECT-begäran. Returnerar det avkodade målet.
async fn negotiate(stream: &mut TcpStream) -> Result<(String, u16), SocksError> {
    let mut greeting_header = [0u8; 2];
    stream.read_exact(&mut greeting_header).await?;
    let (version, nmethods) = (greeting_header[0], greeting_header[1]);
    if version != 5 {
        return Err(SocksError::UnsupportedVersion);
    }
    let mut methods = vec![0u8; nmethods as usize];
    stream.read_exact(&mut methods).await?;
    if !methods.contains(&0x00) {
        return Err(SocksError::NoAcceptableAuthMethod);
    }
    stream.write_all(&[0x05, 0x00]).await?;

    let mut request_header = [0u8; 4];
    stream.read_exact(&mut request_header).await?;
    let (rversion, cmd, atyp) = (request_header[0], request_header[1], request_header[3]);
    if rversion != 5 {
        return Err(SocksError::UnsupportedVersion);
    }
    if cmd != 1 {
        return Err(SocksError::UnsupportedCommand);
    }

    let host = match atyp {
        0x01 => {
            let mut bytes = [0u8; 4];
            stream.read_exact(&mut bytes).await?;
            bytes.iter().map(u8::to_string).collect::<Vec<_>>().join(".")
        }
        0x03 => {
            let mut len_byte = [0u8; 1];
            stream.read_exact(&mut len_byte).await?;
            let mut domain = vec![0u8; len_byte[0] as usize];
            stream.read_exact(&mut domain).await?;
            String::from_utf8_lossy(&domain).into_owned()
        }
        0x04 => {
            let mut bytes = [0u8; 16];
            stream.read_exact(&mut bytes).await?;
            (0..16)
                .step_by(2)
                .map(|i| format!("{:02x}{:02x}", bytes[i], bytes[i + 1]))
                .collect::<Vec<_>>()
                .join(":")
        }
        _ => return Err(SocksError::UnsupportedAddressType),
    };

    let mut port_bytes = [0u8; 2];
    stream.read_exact(&mut port_bytes).await?;
    let port = u16::from_be_bytes(port_bytes);

    Ok((host, port))
}


// MARK: SOCKS5-KLIENT
//
// Motsatt riktning mot resten av modulen: här är Bastion den som ANSLUTER
// genom någon annans proxy, i stället för att vara proxyn.
//
// Behövs för VISION.md "Native WireGuard/Tailscale — inget externt beroende".
// `tailscaled --tun=userspace-networking --socks5-server=<adress>` exponerar
// hela tailnet:et som en SOCKS5-proxy UTAN att kräva root eller ett
// TUN-gränssnitt (flaggorna verifierade direkt mot binären 1.102.2). Det är
// exakt rätt form för Bastion, som bara behöver sina EGNA anslutningar över
// tailnet:et — inte att dirigera om hela systemets trafik. Men för att nyttja
// det måste vi kunna ringa UT genom en SOCKS5-proxy, och det kunde vi inte.

/// Öppnar en TCP-anslutning till `target_host:target_port` GENOM en
/// SOCKS5-proxy på `proxy_addr`.
///
/// Bara CONNECT och "ingen autentisering" — samma avgränsning som modulens
/// serversida, och allt `tailscaled` erbjuder.
///
/// Målet skickas som DOMÄNNAMN (ATYP 0x03) när det inte är en IP-adress, så
/// namnuppslagningen sker i PROXYN. Det är hela poängen med en tailnet-proxy:
/// `min-server.tailnet.ts.net` betyder ingenting för vår lokala resolver, men
/// allt för den i andra änden. Slår vi upp namnet själva först får vi ett fel
/// i stället för en anslutning.
pub async fn connect_via_socks5(
    proxy_addr: &str,
    target_host: &str,
    target_port: u16,
) -> Result<TcpStream, String> {
    let mut stream = TcpStream::connect(proxy_addr)
        .await
        .map_err(|e| format!("kunde inte nå SOCKS5-proxyn {proxy_addr}: {e}"))?;

    // Greeting: version 5, en metod, "ingen autentisering".
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| format!("kunde inte skicka SOCKS5-hälsning: {e}"))?;
    let mut choice = [0u8; 2];
    stream
        .read_exact(&mut choice)
        .await
        .map_err(|e| format!("proxyn svarade inte på hälsningen: {e}"))?;
    if choice[0] != 0x05 {
        return Err(format!("proxyn talar inte SOCKS5 (version {})", choice[0]));
    }
    if choice[1] != 0x00 {
        // 0xFF = ingen godtagbar metod. Vanligaste orsaken är att proxyn
        // kräver användarnamn/lösenord, vilket vi medvetet inte stöder.
        return Err(format!(
            "proxyn godtog ingen autentiseringsmetod vi kan (fick {:#04x});              Bastion stöder bara proxys utan autentisering",
            choice[1]
        ));
    }

    // CONNECT-begäran.
    let mut request = vec![0x05, 0x01, 0x00];
    match target_host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            request.push(0x01);
            request.extend_from_slice(&ip.octets());
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            request.push(0x04);
            request.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            let bytes = target_host.as_bytes();
            // Längden är EN byte i protokollet — ett längre namn går inte
            // att uttrycka, och att klippa det vore att tyst ansluta någon
            // annanstans.
            let len = u8::try_from(bytes.len())
                .map_err(|_| format!("värdnamnet är för långt för SOCKS5 (max 255 tecken): {target_host}"))?;
            request.push(0x03);
            request.push(len);
            request.extend_from_slice(bytes);
        }
    }
    request.extend_from_slice(&target_port.to_be_bytes());
    stream
        .write_all(&request)
        .await
        .map_err(|e| format!("kunde inte skicka SOCKS5-begäran: {e}"))?;

    let mut reply = [0u8; 4];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|e| format!("proxyn svarade inte på begäran: {e}"))?;
    if reply[1] != 0x00 {
        return Err(format!(
            "proxyn avvisade anslutningen till {target_host}:{target_port}: {}",
            socks5_reply_text(reply[1])
        ));
    }

    // Svarets bundna adress måste läsas bort även om vi inte bryr oss om
    // den — annars ligger den kvar i strömmen och blir de första byten
    // SSH-handskakningen tror är serverns versionssträng.
    match reply[3] {
        0x01 => drain(&mut stream, 4 + 2).await?,
        0x04 => drain(&mut stream, 16 + 2).await?,
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await.map_err(|e| e.to_string())?;
            drain(&mut stream, len[0] as usize + 2).await?;
        }
        other => return Err(format!("proxyn svarade med okänd adresstyp {other:#04x}")),
    }

    Ok(stream)
}

async fn drain(stream: &mut TcpStream, n: usize) -> Result<(), String> {
    let mut buf = vec![0u8; n];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| format!("kunde inte läsa proxyns svarsadress: {e}"))
        .map(|_| ())
}

/// RFC 1928 §6. Ett nummer säger användaren ingenting; "Connection refused"
/// och "Host unreachable" kräver olika åtgärder.
fn socks5_reply_text(code: u8) -> &'static str {
    match code {
        0x01 => "allmänt serverfel",
        0x02 => "anslutningen tillåts inte av proxyns regeluppsättning",
        0x03 => "nätverket är inte nåbart",
        0x04 => "värden är inte nåbar",
        0x05 => "anslutningen nekades",
        0x06 => "TTL gick ut",
        0x07 => "kommandot stöds inte",
        0x08 => "adresstypen stöds inte",
        _ => "okänd felkod",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{Host, HostAuth};

    /// Startar en FRISTÅENDE, minimal sshd-instans på en slumpad hög port —
    /// exakt samma teknik/uppsättning som `port_forward::tests::TestSshd`,
    /// men egen kopia här för att slippa göra den delade över modulgränsen
    /// bara för testbruk.
    struct TestSshd {
        child: std::process::Child,
        port: u16,
        dir: std::path::PathBuf,
    }

    impl TestSshd {
        fn start() -> Option<Self> {
            let dir = std::env::temp_dir().join(format!("bastion-socks-sshd-{}", uuid::Uuid::new_v4()));
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
                     AllowTcpForwarding yes\nPidFile {}\n",
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

    fn spawn_echo_server() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                use std::io::{Read, Write};
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

    /// Genuin SOCKS5-klient (RFC 1928 på trådnivå, inte en mockad/förkortad
    /// variant) mot en ekoserver som SOCKS-klienten själv väljer per
    /// anslutning — bevisar hela kedjan: SOCKS-handskakning → dynamiskt vald
    /// `direct-tcpip`-kanal → riktig sshd → oberoende ekoserver → samma väg
    /// tillbaka.
    #[tokio::test]
    async fn dynamic_forward_proxies_a_real_socks5_client_to_a_chosen_echo_server() {
        let Some(sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let echo_port = spawn_echo_server();

        let mut host = Host::new("socks-test".into(), "127.0.0.1".into(), whoami_user());
        host.port = sshd.port as i64;
        host.auth = HostAuth::KeyFile(sshd.client_key_path());

        let rx = spawn_dynamic_forward(host, None, "127.0.0.1".into(), 0, None);
        let forward = rx.recv().await.expect("kanalen stängdes utan svar").expect("forward misslyckades starta");
        assert_ne!(forward.actual_bind_port, 0);

        let mut client = TcpStream::connect(("127.0.0.1", forward.actual_bind_port)).await.unwrap();

        // Greeting: version 5, ett metodval (0x00 = ingen autentisering).
        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut greeting_reply = [0u8; 2];
        client.read_exact(&mut greeting_reply).await.unwrap();
        assert_eq!(greeting_reply, [0x05, 0x00]);

        // CONNECT mot 127.0.0.1:<echo_port>, adresstyp IPv4 (0x01) — klienten
        // väljer det här målet SJÄLV, i farten, precis som en riktig
        // SOCKS5-webbläsarklient skulle göra.
        let mut request = vec![0x05, 0x01, 0x00, 0x01];
        request.extend_from_slice(&[127, 0, 0, 1]);
        request.extend_from_slice(&echo_port.to_be_bytes());
        client.write_all(&request).await.unwrap();
        let mut connect_reply = [0u8; 10];
        client.read_exact(&mut connect_reply).await.unwrap();
        assert_eq!(connect_reply[1], 0x00, "SOCKS CONNECT ska lyckas");

        client.write_all(b"hej-genom-socks5-tunneln").await.unwrap();
        let mut buf = [0u8; 64];
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), client.read(&mut buf))
            .await
            .expect("timeout väntade på eko")
            .expect("läsfel");
        assert_eq!(&buf[..n], b"hej-genom-socks5-tunneln");

        forward.stop();
    }

    fn whoami_user() -> String {
        crate::test_support::test_user()
    }

    // MARK: SOCKS5-klienten

    /// Startar en SOCKS5-stubbserver som svarar EXAKT det testet ber om.
    /// Behövs för felvägarna: Bastions egen server svarar aldrig "fel
    /// version" eller "autentisering krävs", så de grenarna går inte att nå
    /// genom den.
    ///
    /// `greeting_reply` skickas efter hälsningen; `connect_reply` efter
    /// CONNECT. Sedan ekar den allt.
    fn spawn_socks_stub(greeting_reply: Vec<u8>, connect_reply: Vec<u8>) -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let Ok((mut sock, _)) = listener.accept() else { return };
            // Hälsning: 2 byte huvud + nmethods.
            let mut head = [0u8; 2];
            if sock.read_exact(&mut head).is_err() { return; }
            let mut methods = vec![0u8; head[1] as usize];
            let _ = sock.read_exact(&mut methods);
            if sock.write_all(&greeting_reply).is_err() { return; }
            if greeting_reply.get(1) != Some(&0x00) { return; }

            // CONNECT: 4 byte huvud, sedan adress efter typ, sedan port.
            let mut req = [0u8; 4];
            if sock.read_exact(&mut req).is_err() { return; }
            let addr_len = match req[3] {
                0x01 => 4,
                0x04 => 16,
                0x03 => {
                    let mut l = [0u8; 1];
                    if sock.read_exact(&mut l).is_err() { return; }
                    l[0] as usize
                }
                _ => return,
            };
            let mut rest = vec![0u8; addr_len + 2];
            let _ = sock.read_exact(&mut rest);
            if sock.write_all(&connect_reply).is_err() { return; }

            let mut buf = [0u8; 1024];
            while let Ok(n) = sock.read(&mut buf) {
                if n == 0 || sock.write_all(&buf[..n]).is_err() { return; }
            }
        });
        port
    }

    /// Kärnan: vår EGEN klient mot vår EGEN server, genom en riktig SSH-tunnel
    /// till en riktig ekoserver. Båda halvorna talar RFC 1928, och det här är
    /// enda sättet att bevisa att de talar SAMMA RFC 1928.
    ///
    /// Motsvarar hur Bastion kommer nå en tailnet-värd: `tailscaled` kör med
    /// `--socks5-server`, och vi ringer ut genom den.
    #[tokio::test]
    async fn the_client_reaches_a_real_echo_server_through_our_own_socks5_server() {
        let Some(sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let echo_port = spawn_echo_server();

        let mut host = Host::new("socks-klient".into(), "127.0.0.1".into(), whoami_user());
        host.port = sshd.port as i64;
        host.auth = HostAuth::KeyFile(sshd.client_key_path());

        let rx = spawn_dynamic_forward(host, None, "127.0.0.1".into(), 0, None);
        let forward = rx.recv().await.unwrap().expect("forward misslyckades starta");
        let proxy = format!("127.0.0.1:{}", forward.actual_bind_port);

        let mut stream = connect_via_socks5(&proxy, "127.0.0.1", echo_port)
            .await
            .expect("klienten kom inte igenom vår egen proxy");

        stream.write_all(b"tur-och-retur").await.unwrap();
        let mut buf = [0u8; 64];
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut buf))
            .await
            .expect("timeout väntade på eko")
            .expect("läsfel");
        assert_eq!(
            &buf[..n], b"tur-och-retur",
            "svarsadressen måste läsas bort helt — annars blir dess byte det första \
             en SSH-handskakning tror är serverns versionssträng"
        );

        forward.stop();
    }

    /// Ett värdnamn ska skickas som DOMÄN (0x03) och slås upp i PROXYN.
    /// Slår vi upp det själva först får vi ett fel för varje tailnet-namn,
    /// eftersom `min-server.tailnet.ts.net` inte betyder något lokalt.
    #[tokio::test]
    async fn a_host_name_is_sent_as_a_domain_for_the_proxy_to_resolve() {
        let port = spawn_socks_stub(vec![0x05, 0x00], vec![0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
        let mut stream = connect_via_socks5(
            &format!("127.0.0.1:{port}"),
            "namn-som-inte-gar-att-sla-upp.invalid",
            22,
        )
        .await
        .expect("ett ouppslagsbart namn ska ändå gå att BEGÄRA — proxyn avgör");
        stream.write_all(b"x").await.unwrap();
        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"x");
    }

    /// Svarets bundna adress varierar i längd med adresstypen. Läses fel
    /// antal byte bort hamnar resten i strömmen och förgiftar allt efteråt —
    /// det syns som en obegriplig SSH-handskakning, inte som ett SOCKS-fel.
    #[tokio::test]
    async fn every_reply_address_type_is_consumed_completely() {
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("IPv4", vec![0x05, 0x00, 0x00, 0x01, 1, 2, 3, 4, 0, 22]),
            ("IPv6", {
                let mut r = vec![0x05, 0x00, 0x00, 0x04];
                r.extend_from_slice(&[0u8; 16]);
                r.extend_from_slice(&[0, 22]);
                r
            }),
            ("domän", {
                let mut r = vec![0x05, 0x00, 0x00, 0x03, 3];
                r.extend_from_slice(b"abc");
                r.extend_from_slice(&[0, 22]);
                r
            }),
        ];
        for (name, reply) in cases {
            let port = spawn_socks_stub(vec![0x05, 0x00], reply);
            let mut stream = connect_via_socks5(&format!("127.0.0.1:{port}"), "1.2.3.4", 22)
                .await
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            stream.write_all(b"P").await.unwrap();
            let mut buf = [0u8; 1];
            stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"P", "{name}: svarsadressen lästes inte bort helt");
        }
    }

    /// Felkoderna ska bli begripliga meddelanden. "Nekades" och "inte nåbar"
    /// kräver olika åtgärder av användaren; ett nummer kräver en sökmotor.
    #[tokio::test]
    async fn a_rejected_connection_explains_which_rejection_it_was() {
        for (code, expected) in [(0x05u8, "nekades"), (0x04, "inte nåbar"), (0x02, "regeluppsättning")] {
            let port = spawn_socks_stub(vec![0x05, 0x00], vec![0x05, code, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
            let err = connect_via_socks5(&format!("127.0.0.1:{port}"), "1.2.3.4", 22)
                .await
                .expect_err("en icke-noll svarskod är ett fel");
            assert!(err.contains(expected), "kod {code:#04x} gav {err:?}");
        }
    }

    /// En proxy som kräver autentisering ska ge ett fel som SÄGER det, inte
    /// en hängd anslutning eller ett obegripligt protokollfel längre fram.
    #[tokio::test]
    async fn a_proxy_demanding_authentication_says_so() {
        let port = spawn_socks_stub(vec![0x05, 0xFF], vec![]);
        let err = connect_via_socks5(&format!("127.0.0.1:{port}"), "1.2.3.4", 22)
            .await
            .expect_err("0xFF betyder ingen godtagbar metod");
        assert!(err.contains("autentisering"), "fick {err:?}");
    }

    /// Något som inte är en SOCKS5-proxy alls.
    #[tokio::test]
    async fn a_server_that_is_not_socks5_is_reported_as_such() {
        let port = spawn_socks_stub(vec![0x04, 0x00], vec![]);
        let err = connect_via_socks5(&format!("127.0.0.1:{port}"), "1.2.3.4", 22)
            .await
            .expect_err("version 4 är inte SOCKS5");
        assert!(err.contains("SOCKS5"), "fick {err:?}");
    }

    /// Längden på ett domännamn är EN byte i protokollet. Ett längre namn går
    /// inte att uttrycka, och att klippa det vore att tyst ansluta till en
    /// annan värd än den användaren bad om.
    #[tokio::test]
    async fn a_host_name_too_long_for_the_protocol_is_refused_not_truncated() {
        let port = spawn_socks_stub(vec![0x05, 0x00], vec![0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
        let long = "a".repeat(256);
        let err = connect_via_socks5(&format!("127.0.0.1:{port}"), &long, 22)
            .await
            .expect_err("256 tecken ryms inte i en längdbyte");
        assert!(err.contains("för långt"), "fick {err:?}");
    }
}
