//! SSH-anslutning via russh, körs på en egen bakgrundstråd (egen
//! single-thread tokio-runtime) eftersom GTK:s huvudloop är glib, inte tokio.
//! Kommunicerar med UI-tråden via `async_channel` (Send+Sync, kan pollas från
//! både tokio och glibs `spawn_local`).
//!
//! Host-key-verifiering: TOFU via `crate::known_hosts::KnownHosts`, samma
//! princip och filformat som Sources/SSHCore/KnownHosts.swift +
//! HostKeyValidator.swift.
//!
//! KÄND BEGRÄNSNING: `HostAuth::KeyFile` (utan lösenfras),
//! `HostAuth::AgentDefault` (ssh-agent), `HostAuth::AskPassword`
//! (lösenord), `HostAuth::CertificateFile` (OpenSSH-certifikat, se nedan)
//! och `HostAuth::BitwardenItem` (se `bitwarden.rs` — LINUX är faktiskt
//! den ENDA plattformen där den fungerar, inte en Rust-specifik lucka)
//! stöds. Bara `HostAuth::KeychainKey` saknar en Linux-motsvarighet
//! (genuint Apple Keychain-specifik).
//!
//! Certifikatautentisering (`HostAuth::CertificateFile`): russh har,
//! till skillnad från swift-nio-ssh (se `ROADMAP.md`s notering om att
//! NIOSSH-SERVERrollen inte kan TA EMOT cert-auth — irrelevant för oss
//! som alltid är klient, men det gjorde att Swift-sidans egna tester
//! aldrig kunde bevisa en fullständig nätverksrundtur), FÖRSTKLASSIGT
//! stöd för att en klient ERBJUDER ett OpenSSH-certifikat
//! (`Handle::authenticate_openssh_cert`, `russh::keys::
//! load_openssh_certificate`) — inget eget protokollarbete behövs här
//! heller.

use crate::host::{Host, HostAuth};
use crate::known_hosts::{KnownHosts, Verdict};
use russh::client::Msg;
use russh::client::{self, Handle};
use russh::keys::agent::client::AgentClient;
use russh::keys::ssh_key::PublicKey;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyBase64, load_secret_key};
use russh::{Channel, ChannelMsg};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;

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

/// Delad karta port->mål för fjärr-portvidarebefordran (`-R`), motsvarande
/// `SSHSession.remoteForwards` i SSHCore. Tom för anslutningar som inte
/// använder `-R` (interaktiv shell, engångskommandon, `-L`) — bara
/// `spawn_remote_forward` (`port_forward.rs`) fyller på den.
pub(crate) type RemoteForwards = Arc<Mutex<HashMap<u32, (String, u16)>>>;

pub(crate) struct ClientHandler {
    host: String,
    port: u16,
    known_hosts: Arc<KnownHosts>,
    remote_forwards: RemoteForwards,
}

impl client::Handler for ClientHandler {
    type Error = ConnectError;

    async fn check_server_key(&mut self, server_public_key: &PublicKey) -> Result<bool, Self::Error> {
        let key_string = format!(
            "{} {}",
            server_public_key.algorithm().as_str(),
            server_public_key.public_key_base64()
        );
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

    /// Motsvarar `handleInboundForwardedChannel` i SSHCore/PortForward.swift:
    /// servern öppnar den här kanalen när en klient ansluter mot en port vi
    /// bad den lyssna på via `tcpip_forward` (`spawn_remote_forward`). Porten
    /// slås upp i `remote_forwards` för att hitta den LOKALA
    /// host:port-anslutningen som ska bryggas mot — allt annat (ingen aktiv
    /// `-R` för den porten) släpps tyst, samma som SSHCore avvisar det.
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        _connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        // NY OCH OBLIGATORISK i russh 0.62: kanalöppningen måste
        // uttryckligen accepteras eller avvisas. Att bara låta handtaget
        // droppas skickar automatiskt `AdministrativelyProhibited` —
        // vilket i praktiken stängde varje vidarebefordrad anslutning
        // ("Connection reset by peer"). Fångat av `-R`-testet mot en
        // riktig sshd under uppgraderingen från 0.45.
        handle: russh::ChannelOpenHandleInner<Msg>,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let target = self
            .remote_forwards
            .lock()
            .expect("remote_forwards-låset korrupt")
            .get(&connected_port)
            .cloned();
        let Some((target_host, target_port)) = target else {
            // Ingen aktiv `-R` för den porten — avvisa uttryckligen,
            // samma utfall som tidigare fast nu explicit uttryckt.
            handle.reject(russh::ChannelOpenFailure::AdministrativelyProhibited).await;
            return Ok(());
        };
        handle.accept().await;
        tokio::spawn(async move {
            let local = match TcpStream::connect((target_host.as_str(), target_port)).await {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut local = local;
            let mut remote = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
        });
        Ok(())
    }
}

/// Startar SSH-anslutningen på en ny bakgrundstråd och returnerar kanalerna
/// direkt — anropas från GTK-huvudtråden, blockerar inte den.
pub fn spawn_shell(
    host: Host,
    password: Option<String>,
    cols: u32,
    rows: u32,
    jump: Option<Host>,
) -> SshSession {
    let (input_tx, input_rx) = async_channel::unbounded::<Vec<u8>>();
    let (output_tx, output_rx) = async_channel::unbounded::<SshEvent>();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för SSH-tråden");
        rt.block_on(async move {
            if let Err(e) = run(
                host,
                password,
                cols,
                rows,
                input_rx,
                output_tx.clone(),
                None,
                jump,
            )
            .await
            {
                let _ = output_tx.send(SshEvent::Error(e)).await;
            }
            let _ = output_tx.send(SshEvent::Closed).await;
        });
    });

    SshSession {
        input: input_tx,
        output: output_rx,
    }
}

/// Hur länge `client::connect` (TCP + SSH-handskakning) får ta innan den
/// ges upp — utan detta kan en obesvarad/svarthålsad värd blockera hela
/// bakgrundstråden (och därmed den väntande UI-kanalen) på obestämd tid.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// Hur länge ett engångskommando (`run_command_once`, Docker-listor/-loggar
/// m.fl.) får köra innan det avbryts — samma resonemang som `CONNECT_TIMEOUT`,
/// fast för fjärrkommandot självt (en hängande shell/process på fjärrsidan).
pub(crate) const COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// Tak på hur mycket utdata ett engångskommando ackumulerar i minnet —
/// `docker logs` utan `--tail` eller en oavsiktlig `cat` av en stor fil ska
/// inte kunna svälta GUI-processen. 4 MiB räcker gott för det här
/// användningsfallet (statuslistor/loggutdrag), inte en generell
/// filöverföringskanal (den finns redan, SFTP).
const MAX_COMMAND_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// Ansluter och autentiserar — delad av den interaktiva shell-sessionen
/// (`run`) och engångskommandon (`run_command_once`, t.ex. Docker-anrop).
/// `jump`: se `connect_with_forwards`.
pub(crate) async fn connect(
    host: &Host,
    password: Option<String>,
    known_hosts_path_override: Option<std::path::PathBuf>,
    jump: Option<Host>,
) -> Result<Handle<ClientHandler>, String> {
    connect_with_forwards(
        host,
        password,
        known_hosts_path_override,
        RemoteForwards::default(),
        jump,
    )
    .await
}

/// Samma som `connect`, men tar en delad `RemoteForwards`-karta som
/// `ClientHandler` slår upp mot när servern öppnar en `forwarded-tcpip`-kanal
/// — `connect()` skickar bara med en tom (oanvänd) karta, bara
/// `spawn_remote_forward` (`port_forward.rs`) behöver fylla på den efteråt.
///
/// `jump`: motsvarar `SSHSession.connect(via:)`/`SSHConnectionChain` i Swift
/// (`ssh -J`/ProxyJump) — redan UPPLÖST mot en riktig `Host` av anroparen
/// (`host::HostStore::resolve_jump`, som även avvisar kedjor med mer än ett
/// hopp). `None` betyder en vanlig direktanslutning.
pub(crate) async fn connect_with_forwards(
    host: &Host,
    password: Option<String>,
    known_hosts_path_override: Option<std::path::PathBuf>,
    remote_forwards: RemoteForwards,
    jump: Option<Host>,
) -> Result<Handle<ClientHandler>, String> {
    // Faller stängt: går known_hosts-filen inte att läsa avbryts
    // anslutningen hellre än att fortsätta utan MITM-skydd (se
    // `KnownHosts::load`).
    let known_hosts = Arc::new(
        KnownHosts::open(Some(
            known_hosts_path_override
                .clone()
                .unwrap_or_else(KnownHosts::default_path),
        ))
        .map_err(|e| format!("kunde inte läsa known_hosts (vägrar ansluta utan värdnyckelskontroll): {e}"))?,
    );
    let target_handler = ClientHandler {
        host: host.host_name.clone(),
        port: host.port as u16,
        known_hosts,
        remote_forwards,
    };

    let mut session: Handle<ClientHandler> = match jump {
        None => connect_direct(host, target_handler).await?,
        Some(jump_host) => {
            connect_via_jump(&jump_host, host, target_handler, known_hosts_path_override).await?
        }
    };
    authenticate(&mut session, host, password).await?;
    Ok(session)
}

/// Direktanslutningen (TCP + SSH-handskaka), UTAN jump-gren — utbruten ur
/// `connect_with_forwards` så att `connect_via_jump` kan återanvända EXAKT
/// samma logik för att ansluta till jump-hosten SJÄLV, utan att `connect`/
/// `connect_with_forwards`/`connect_via_jump` bildar en (statiskt sett
/// oändlig) rekursiv async-fn-cykel — `rustc` kan inte bevisa att
/// `connect_via_jump`s eget anrop alltid skickar `jump: None` och därmed
/// aldrig faktiskt rekurserar i praktiken (E0733, "recursion in an async fn
/// requires boxing"), så cykeln bryts strukturellt istället för via
/// `Box::pin`.
async fn connect_direct(
    host: &Host,
    handler: ClientHandler,
) -> Result<Handle<ClientHandler>, String> {
    let config = Arc::new(client::Config::default());
    let addr = (host.host_name.as_str(), host.port as u16);
    tokio::time::timeout(CONNECT_TIMEOUT, client::connect(config, addr, handler))
        .await
        .map_err(|_| {
            format!(
                "anslutningen svarade inte inom {}s",
                CONNECT_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("anslutning misslyckades: {e}"))
}

/// Ansluter GENOM en redan uppkopplad jump-host (`ssh -J`/ProxyJump) —
/// motsvarar `SSHSession.connect(via:)` i SSHCore. `jump_host` autentiseras
/// FÖRST (helt separat handskakning/TOFU-koll mot SIG SJÄLV), sedan öppnas
/// en `direct-tcpip`-kanal från jump-sessionen till `target_host`s
/// `host_name:port` — och en HELT NY, oberoende SSH-handskakning
/// (`client::connect_stream`, `target_handler`s EGEN TOFU-koll mot target)
/// körs direkt ovanpå den kanalens byteström. Samma "SSH i SSH"-mönster som
/// en riktig `ssh -J` gör på trådnivå.
///
/// Jump-hosten autentiseras UTAN lösenordsprompt (`connect(jump_host, None,
/// …)`) — samma begränsning som `App/AuthResolver.resolveConnectionPlan`
/// (`resolveAuth(for: jumpHost, password: nil)`): en jump-host som kräver
/// `AskPassword`-auth kan inte användas som hopp i dagsläget (misslyckas
/// tydligt via `authenticate`s "lösenord krävs men saknades" nedan), bara
/// nyckel-/agent-baserad auth stöds för SJÄLVA HOPPET. Target-hosten kan
/// fortfarande fråga efter lösenord som vanligt.
///
/// Jump-sessionens `Handle` behöver INTE hållas vid liv explicit efter att
/// kanalen öppnats: `Channel::into_stream()` ger en `ChannelStream` som
/// håller sin EGEN klon av sessionens interna sändare (russh dokumenterar
/// `Channel` som "allows you to read and write from a channel without
/// borrowing the session") — russh:s bakgrundstråd för jump-anslutningen
/// fortsätter därför vidarebefordra tunnelns data så länge kanalen används,
/// oavsett att `jump_session` går ur scope här. `drop(jump_session)` nedan
/// är därför medvetet, inte en läcka.
async fn connect_via_jump(
    jump_host: &Host,
    target_host: &Host,
    target_handler: ClientHandler,
    known_hosts_path_override: Option<std::path::PathBuf>,
) -> Result<Handle<ClientHandler>, String> {
    // Samma "fall stängt"-regel som i `connect_direct` ovan.
    let jump_known_hosts = Arc::new(
        KnownHosts::open(Some(known_hosts_path_override.unwrap_or_else(KnownHosts::default_path)))
            .map_err(|e| format!("kunde inte läsa known_hosts för jump-hosten (vägrar ansluta utan värdnyckelskontroll): {e}"))?,
    );
    let jump_handler = ClientHandler {
        host: jump_host.host_name.clone(),
        port: jump_host.port as u16,
        known_hosts: jump_known_hosts,
        remote_forwards: RemoteForwards::default(),
    };
    let mut jump_session = connect_direct(jump_host, jump_handler)
        .await
        .map_err(|e| format!("kunde inte ansluta till jump-hosten \"{}\": {e}", jump_host.alias))?;
    authenticate(&mut jump_session, jump_host, None)
        .await
        .map_err(|e| format!("autentisering mot jump-hosten \"{}\" misslyckades: {e}", jump_host.alias))?;

    let channel = jump_session
        .channel_open_direct_tcpip(
            target_host.host_name.clone(),
            target_host.port as u32,
            "127.0.0.1",
            0,
        )
        .await
        .map_err(|e| {
            format!(
                "kunde inte öppna en tunnel genom jump-hosten \"{}\": {e}",
                jump_host.alias
            )
        })?;
    let stream = channel.into_stream();

    let config = Arc::new(client::Config::default());
    let target_session = tokio::time::timeout(
        CONNECT_TIMEOUT,
        client::connect_stream(config, stream, target_handler),
    )
    .await
    .map_err(|_| {
        format!(
            "anslutningen genom jump-hosten \"{}\" svarade inte inom {}s",
            jump_host.alias,
            CONNECT_TIMEOUT.as_secs()
        )
    })?
    .map_err(|e| {
        format!(
            "anslutning genom jump-hosten \"{}\" misslyckades: {e}",
            jump_host.alias
        )
    })?;

    drop(jump_session);
    Ok(target_session)
}

/// Kör ETT kommando över en fristående anslutning (ingen pty, ingen
/// interaktiv shell) och returnerar stdout+stderr som text. Används för
/// engångsanrop (Docker list/start/stopp/loggar) — en ny anslutning per
/// anrop är enklare och korrekt, om än inte det mest effektiva; se
/// ROADMAP.md om det senare visar sig behöva en delad uppkopplad session.
pub fn run_command(
    host: Host,
    password: Option<String>,
    command: String,
    jump: Option<Host>,
) -> async_channel::Receiver<Result<String, String>> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för kommandotråden");
        let result = rt.block_on(run_command_once(host, password, command, None, jump));
        let _ = tx.send_blocking(result);
    });
    rx
}

async fn run_command_once(
    host: Host,
    password: Option<String>,
    command: String,
    known_hosts_path_override: Option<std::path::PathBuf>,
    jump: Option<Host>,
) -> Result<String, String> {
    let session = connect(&host, password, known_hosts_path_override, jump).await?;
    tokio::time::timeout(COMMAND_TIMEOUT, run_command_on_session(&session, &command))
        .await
        .map_err(|_| format!("kommandot svarade inte inom {}s", COMMAND_TIMEOUT.as_secs()))?
}

async fn run_command_on_session(
    session: &Handle<ClientHandler>,
    command: &str,
) -> Result<String, String> {
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("kunde inte öppna kanal: {e}"))?;
    channel
        .exec(true, command.as_bytes())
        .await
        .map_err(|e| format!("kommandot kunde inte köras: {e}"))?;

    let mut output = Vec::new();
    let mut truncated = false;
    // VIKTIGT: bryt INTE på `ExitStatus`. SSH garanterar inte att all
    // `Data` hunnit levereras innan `exit-status` — servern skickar
    // typiskt `exit-status` direkt när processen dör, medan utdata
    // fortfarande kan ligga kvar i kanalens kö. En tidigare version
    // gjorde `ExitStatus => break` och tappade då utdatan helt när
    // meddelandena råkade komma i den ordningen: kommandot "lyckades"
    // men returnerade tom sträng.
    //
    // Det syntes som ett flakigt `connect_via_jump_reaches_the_real_
    // separate_target_sshd` i CI (två gånger), men var i själva verket
    // en RIKTIG bugg som drabbar allt som läser kommandoutdata —
    // systemöversikten, Docker-listan, Tailscale-hämtning över SSH —
    // med tom vy som resultat. Lastberoende, därav "flakigt".
    //
    // `Eof`/`Close` (eller att `wait()` ger `None`) är de enda korrekta
    // slutvillkoren: efter `Eof` kommer per definition ingen mer data.
    // `COMMAND_TIMEOUT` i `run_command_once` skyddar mot en server som
    // aldrig stänger kanalen.
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                if output.len() < MAX_COMMAND_OUTPUT_BYTES {
                    let remaining = MAX_COMMAND_OUTPUT_BYTES - output.len();
                    output.extend_from_slice(&data[..data.len().min(remaining)]);
                    if data.len() > remaining {
                        truncated = true;
                    }
                } else {
                    truncated = true;
                }
            }
            ChannelMsg::Eof | ChannelMsg::Close => break,
            _ => {}
        }
    }
    let mut text =
        String::from_utf8(output).map_err(|e| format!("ogiltig UTF-8 i kommandots utdata: {e}"))?;
    if truncated {
        text.push_str(&format!(
            "\n[...avkortad, mer än {} MiB utdata...]",
            MAX_COMMAND_OUTPUT_BYTES / (1024 * 1024)
        ));
    }
    Ok(text)
}

async fn run(
    host: Host,
    password: Option<String>,
    cols: u32,
    rows: u32,
    input_rx: async_channel::Receiver<Vec<u8>>,
    output_tx: async_channel::Sender<SshEvent>,
    known_hosts_path_override: Option<std::path::PathBuf>,
    jump: Option<Host>,
) -> Result<(), String> {
    let session = connect(&host, password, known_hosts_path_override, jump).await?;

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
            let key = load_secret_key(path, None).map_err(|e| {
                format!("kunde inte läsa nyckelfilen {path}: {e} (lösenfraser stöds inte än)")
            })?;
            // `PrivateKeyWithHashAlg`: russh 0.62 kräver ett uttryckligt
            // hash-val för RSA-nycklar (ssh-rsa/rsa-sha2-256/-512).
            // `best_supported_rsa_hash` frågar servern vad den klarar;
            // för Ed25519/ECDSA ignoreras värdet helt.
            let hash_alg = session
                .best_supported_rsa_hash()
                .await
                .map_err(|e| format!("kunde inte förhandla hash-algoritm: {e}"))?
                .flatten();
            session
                .authenticate_publickey(&host.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg))
                .await
                .map_err(|e| format!("publik nyckel-autentisering misslyckades: {e}"))?
                .success()
        }
        HostAuth::AgentDefault => {
            let mut agent = AgentClient::connect_env()
                .await
                .map_err(|e| format!("kunde inte ansluta till ssh-agent: {e}"))?;
            let identities = agent
                .request_identities()
                .await
                .map_err(|e| format!("kunde inte hämta identiteter från ssh-agent: {e}"))?;
            if identities.is_empty() {
                return Err("ssh-agent har inga laddade identiteter".into());
            }
            // En ssh-agent kan ha flera laddade nycklar — precis som riktig
            // `ssh` provas var och en i tur och ordning tills en lyckas,
            // istället för att ge upp bara för att den FÖRSTA inte råkar
            // vara den servern accepterar (CodeRabbit-fynd).
            // `authenticate_future` ersattes i russh 0.62 av
            // `authenticate_publickey_with`, som lånar agenten i stället
            // för att flytta in och tillbaka den — loopen blir enklare.
            let mut succeeded = false;
            for identity in identities {
                // `request_identities` ger nu `AgentIdentity` (nyckel +
                // kommentar, eller ett certifikat). Bara rena publika
                // nycklar används här — certifikat via agent har en egen
                // väg (`HostAuth::CertificateFile`).
                let russh::keys::agent::AgentIdentity::PublicKey { key, .. } = identity else {
                    continue;
                };
                let hash_alg = if key.algorithm().is_rsa() { Some(HashAlg::Sha256) } else { None };
                let result = session
                    .authenticate_publickey_with(&host.user, key, hash_alg, &mut agent)
                    .await;
                if matches!(result, Ok(ref r) if r.success()) {
                    succeeded = true;
                    break;
                }
            }
            succeeded
        }
        HostAuth::AskPassword => {
            let pass = password.ok_or("lösenord krävs men saknades")?;
            session
                .authenticate_password(&host.user, pass)
                .await
                .map_err(|e| format!("lösenordsautentisering misslyckades: {e}"))?
                .success()
        }
        HostAuth::CertificateFile { key_path, cert_path } => {
            let key = load_secret_key(key_path, None).map_err(|e| {
                format!("kunde inte läsa nyckelfilen {key_path}: {e} (lösenfraser stöds inte än)")
            })?;
            let cert = russh::keys::load_openssh_certificate(cert_path)
                .map_err(|e| format!("kunde inte läsa certifikatfilen {cert_path}: {e}"))?;
            session
                .authenticate_openssh_cert(&host.user, Arc::new(key), cert)
                .await
                .map_err(|e| format!("certifikat-autentisering misslyckades: {e}"))?
                .success()
        }
        HostAuth::BitwardenItem(item_id) => {
            // Till skillnad från Apple-sidan (där `resolveAuth` ALLTID
            // returnerar `nil` för `.bitwardenItem` — iOS saknar
            // `Foundation.Process` helt, macOS App Sandbox dödar `bw`-
            // processen med ett okatchbart SIGTRAP) är Linux den ENDA
            // plattformen där det här faktiskt kan fungera, se
            // `bitwarden.rs`s modulkommentar.
            let session_key = std::env::var("BW_SESSION").ok();
            let pass = crate::bitwarden::fetch_password("bw", item_id, session_key.as_deref())
                .map_err(|e| format!("kunde inte hämta lösenord från Bitwarden: {e}"))?;
            session
                .authenticate_password(&host.user, pass)
                .await
                .map_err(|e| format!("lösenordsautentisering misslyckades: {e}"))?
                .success()
        }
        other => {
            return Err(format!(
                "autentiseringstypen {other:?} stöds inte på Linux ännu"
            ));
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
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            if let Err(e) = run(
                host,
                password,
                cols,
                rows,
                input_rx,
                output_tx.clone(),
                Some(known_hosts_path),
                None,
            )
            .await
            {
                let _ = output_tx.send(SshEvent::Error(e)).await;
            }
            let _ = output_tx.send(SshEvent::Closed).await;
        });
    });
    SshSession {
        input: input_tx,
        output: output_rx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Host;
    use std::time::Duration;

    fn drain_until_data_error_or_closed(
        session: &SshSession,
        timeout: Duration,
    ) -> Result<(), String> {
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
        let key_path =
            std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let session = spawn_shell(host, None, 80, 24, None);
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
        let key_path =
            std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let known_hosts_path = std::env::temp_dir().join(format!(
            "bastion-tofu-test-{}.known_hosts",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(
            &known_hosts_path,
            "127.0.0.1:22 ssh-ed25519 FALSKT-INTE-DEN-RIKTIGA-NYCKELN\n",
        )
        .unwrap();

        let session = spawn_shell_with_known_hosts(host, None, 80, 24, known_hosts_path.clone());
        let result = drain_until_data_error_or_closed(&session, Duration::from_secs(10));
        std::fs::remove_file(&known_hosts_path).ok();

        match result {
            Err(msg) => assert!(
                msg.contains("HAR ÄNDRATS"),
                "väntade ett host-key-avslag, fick: {msg}"
            ),
            Ok(()) => {
                panic!("anslutningen borde ha avvisats p.g.a. ändrad värdnyckel, men lyckades")
            }
        }
    }

    /// Verifierar `run_command` (engångs-exec, ingen pty) mot en riktig
    /// sshd — LÄSANDE kommando bara (`docker ps`), rör ALDRIG start/stopp
    /// på riktiga containrar som kan köra på testmaskinen.
    #[test]
    #[ignore = "kräver en riktig localhost-sshd + en nyckel förberedd i authorized_keys, se ROADMAP.md"]
    fn run_command_executes_a_real_readonly_command_over_ssh() {
        let key_path =
            std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let rx = run_command(host, None, "echo bastion-run-command-ok".to_string(), None);
        let result = rx.recv_blocking().expect("kanalen stängdes utan svar");
        assert_eq!(result.unwrap().trim(), "bastion-run-command-ok");
    }

    /// Docker-vyns list-kommando mot en riktig `dockerd` med riktiga
    /// containrar — LÄSANDE (`docker ps`) bara, rör aldrig start/stopp/
    /// omstart av testmaskinens faktiska containrar.
    #[test]
    #[ignore = "kräver riktig localhost-sshd + docker + en nyckel i authorized_keys, se ROADMAP.md"]
    fn docker_list_command_parses_real_dockerd_output() {
        let key_path =
            std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let rx = run_command(host, None, crate::docker::list_command(true), None);
        let output = rx
            .recv_blocking()
            .expect("kanalen stängdes utan svar")
            .expect("docker ps misslyckades");
        let containers = crate::docker::parse_list(&output);
        assert!(
            !containers.is_empty(),
            "väntade minst en container på testmaskinen, fick ingen"
        );
    }

    /// Verifierar att skriva `exit` i den interaktiva shellen faktiskt
    /// stänger SSH-sessionen (får `SshEvent::Closed`) — det uttryckliga
    /// kravet "exit måste avsluta sessionen". `main.rs::start_session`
    /// reagerar på just denna händelse genom att stänga fliken.
    #[test]
    #[ignore = "kräver en riktig localhost-sshd + en nyckel förberedd i authorized_keys, se ROADMAP.md"]
    fn typing_exit_in_the_shell_closes_the_session() {
        let key_path =
            std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        let user = std::env::var("USER").expect("USER måste vara satt");
        let mut host = Host::new("test".into(), "127.0.0.1".into(), user);
        host.auth = HostAuth::KeyFile(key_path);

        let session = spawn_shell(host, None, 80, 24, None);
        // Vänta in första skalpromptens data innan vi skriver något, annars
        // kan "exit\n" hamna innan skalet ens är redo att läsa stdin.
        drain_until_data_error_or_closed(&session, Duration::from_secs(10))
            .expect("fick aldrig en initial prompt från skalet");

        session
            .input
            .send_blocking(b"exit\n".to_vec())
            .expect("kunde inte skicka exit till skalet");

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
        assert!(
            closed,
            "sessionen stängdes aldrig efter att exit skrevs i skalet"
        );
    }

    /// Fristående test-sshd (egen konfig/port, INTE systemtjänsten) — samma
    /// teknik som `port_forward`/`socks_proxy`/`key_deploy`, används här så
    /// output-taket kan verifieras utan manuell `authorized_keys`-uppsättning.
    struct TestSshd {
        child: std::process::Child,
        port: u16,
        dir: std::path::PathBuf,
    }

    impl TestSshd {
        fn start() -> Option<Self> {
            let dir = std::env::temp_dir().join(format!(
                "bastion-ssh-output-cap-sshd-{}",
                uuid::Uuid::new_v4()
            ));
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
                     PidFile {}\n",
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

    /// Ett kommando som producerar betydligt mer än `MAX_COMMAND_OUTPUT_BYTES`
    /// ska avkortas (med en tydlig markör), inte svälla minnet obegränsat.
    #[test]
    fn run_command_output_is_capped_not_unbounded() {
        let Some(sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let mut host = Host::new("output-cap-test".into(), "127.0.0.1".into(), whoami_user());
        host.port = sshd.port as i64;
        host.auth = HostAuth::KeyFile(sshd.client_key_path());

        // 6 MiB rå utdata (inte avkortad av något mellanled) — väl över det
        // 4 MiB-taket.
        let rx = run_command(host, None, "yes a | head -c 6291456".to_string(), None);
        let output = rx
            .recv_blocking()
            .expect("kanalen stängdes utan svar")
            .expect("kommandot misslyckades");
        assert!(
            output.len() < 6 * 1024 * 1024,
            "utdatan ska ha avkortats, fick {} bytes",
            output.len()
        );
        assert!(
            output.contains("avkortad"),
            "avkortad utdata ska ha en tydlig markör, fick slutet: {}",
            &output[output.len().saturating_sub(80)..]
        );
    }

    fn whoami_user() -> String {
        std::env::var("USER").unwrap_or_else(|_| "test".into())
    }

    /// Bygger en `Host` som pekar mot en `TestSshd`-instans, med dess egen
    /// klientnyckel som auth.
    fn host_for(sshd: &TestSshd, alias: &str) -> Host {
        let mut host = Host::new(alias.into(), "127.0.0.1".into(), whoami_user());
        host.port = sshd.port as i64;
        host.auth = HostAuth::KeyFile(sshd.client_key_path());
        host
    }

    /// GENUIN ProxyJump-verifiering (`ssh -J`), inte en kortsluten
    /// loopback-gissning: TVÅ HELT OBEROENDE `TestSshd`-instanser (egna
    /// portar, egna värdnycklar, egna klientnyckelpar) — `connect_via_jump`
    /// måste alltså (1) autentisera mot jump-hosten på RIKTIGT, (2) öppna en
    /// äkta `direct-tcpip`-kanal genom den, och (3) köra en HELT SEPARAT
    /// SSH-handskakning+autentisering mot target-sshd:n ÖVER den kanalens
    /// byteström — innan kommandot ens kan exekvera. Motsvarar
    /// `SSHConnectionChain.connect`-testerna i SSHCoreTests, fast mot en
    /// riktig `sshd`-process istället för `LoopbackServer`.
    #[tokio::test]
    async fn connect_via_jump_reaches_the_real_separate_target_sshd() {
        let Some(jump) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let Some(target) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let jump_host = host_for(&jump, "jump");
        let target_host = host_for(&target, "target");

        let session = connect(&target_host, None, None, Some(jump_host))
            .await
            .expect("anslutning genom jump-hosten misslyckades");
        let output = run_command_on_session(&session, "echo bastion-proxyjump-ok")
            .await
            .expect("kommandot över den tunnlade sessionen misslyckades");
        assert_eq!(output.trim(), "bastion-proxyjump-ok");

        // Sanity: target-sshd:n är en genuint egen, självständigt fungerande
        // process — inte bara ett hål som råkar svara p.g.a. jump-hosten.
        // Bevisar att testets "två oberoende servrar"-premiss faktiskt
        // stämmer, inte bara antas.
        let direct = connect(&target_host, None, None, None).await;
        assert!(
            direct.is_ok(),
            "target-sshd:n borde vara nåbar även direkt (utan jump) i den här testmiljön"
        );
    }

    /// Om jump-hosten SJÄLV inte går att autentisera mot (fel nyckel) ska
    /// felet peka tydligt på JUMP-hosten — täcker samma risk som Swifts
    /// `ProxyJumpTests` (se `KeyManagement.swift`s kommentar om
    /// `testConnectionChainClosesJumpWhenTargetAuthFails`): ett fel får
    /// aldrig tystas eller felaktigt tillskrivas fel hopp i kedjan.
    #[tokio::test]
    async fn connect_via_jump_fails_clearly_when_the_jump_cant_authenticate() {
        let Some(jump) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let Some(target) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        // Fel nyckel för jump-hosten (target-sshd:ns egen — giltig NYCKEL,
        // men inte en jump-sshd:n litar på).
        let mut jump_host = host_for(&jump, "jump");
        jump_host.auth = HostAuth::KeyFile(target.client_key_path());
        let target_host = host_for(&target, "target");

        // `Handle<ClientHandler>` implementerar inte `Debug` — `expect_err`
        // kräver det, så felet plockas ut manuellt istället.
        let err = match connect(&target_host, None, None, Some(jump_host)).await {
            Ok(_) => panic!("anslutningen skulle ha misslyckats — jump-hosten avvisar nyckeln"),
            Err(e) => e,
        };
        assert!(
            err.contains("jump-hosten"),
            "felet ska tydligt peka på jump-hosten, fick: {err}"
        );
    }

    /// Om jump-hosten autentiserar rent men target-hosten avvisar nyckeln
    /// SKA felet fortfarande vara tydligt (inte en generisk tunnel-krasch) —
    /// samma distinktion som Swift-sidans kedjelogik gör mellan ett
    /// jump-fel och ett target-fel.
    #[tokio::test]
    async fn connect_via_jump_fails_clearly_when_the_target_cant_authenticate() {
        let Some(jump) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let Some(target) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let jump_host = host_for(&jump, "jump");
        // Fel nyckel för target — jump-hosten sen tidigare bevisat fungera.
        let mut target_host = host_for(&target, "target");
        target_host.auth = HostAuth::KeyFile(jump.client_key_path());

        let err = match connect(&target_host, None, None, Some(jump_host)).await {
            Ok(_) => panic!("anslutningen skulle ha misslyckats — target avvisar nyckeln"),
            Err(e) => e,
        };
        assert!(
            !err.contains("jump-hosten"),
            "felet ska INTE felaktigt tillskrivas jump-hosten (den autentiserade rent), fick: {err}"
        );
    }

    /// Test-sshd konfigurerad för OpenSSH-certifikatautentisering
    /// (`TrustedUserCAKeys`) i stället för `TestSshd`s `AuthorizedKeysFile`
    /// — en helt annan sshd-konfiguration, så en egen struct i stället för
    /// att grena `TestSshd`s `start()` på ett flaggargument.
    struct TestCertSshd {
        child: std::process::Child,
        port: u16,
        dir: std::path::PathBuf,
    }

    impl TestCertSshd {
        /// `trusted_ca_pub`: den CA-publiknyckel sshd litar på. Ingen
        /// `AuthorizedKeysFile` alls — bara certifikat signerade av denna
        /// CA (och med en principal som matchar den efterfrågade
        /// inloggningsanvändaren, sshds standardbeteende utan en
        /// `AuthorizedPrincipalsFile`) accepteras.
        fn start(trusted_ca_pub: &std::path::Path) -> Option<Self> {
            let dir = std::env::temp_dir().join(format!(
                "bastion-ssh-cert-sshd-{}",
                uuid::Uuid::new_v4()
            ));
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

            let port = crate::test_support::reserve_port()?;
            let config_path = dir.join("sshd_config");
            std::fs::write(
                &config_path,
                format!(
                    "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nTrustedUserCAKeys {}\n\
                     PubkeyAuthentication yes\nPasswordAuthentication no\nUsePAM no\nStrictModes no\n\
                     PidFile {}\n",
                    host_key.display(),
                    trusted_ca_pub.display(),
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
            Some(TestCertSshd { child, port, dir })
        }
    }

    impl Drop for TestCertSshd {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Genererar ett CA-nyckelpar + ett användarnyckelpar i `dir` och
    /// signerar det senare med det förra (`ssh-keygen -s`, RIKTIGA
    /// nycklar/signaturer — samma verktyg riktig OpenSSH-drift använder,
    /// inget eget certifikatbygge). Returnerar
    /// `(ca_pub_path, user_key_path, user_cert_path)`.
    fn make_ca_and_signed_cert(
        dir: &std::path::Path,
        principal: &str,
    ) -> Option<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)> {
        let ca_key = dir.join("ca_key");
        if !std::process::Command::new("ssh-keygen")
            .args(["-q", "-N", "", "-t", "ed25519", "-f"])
            .arg(&ca_key)
            .status()
            .ok()?
            .success()
        {
            return None;
        }
        let user_key = dir.join("user_key");
        if !std::process::Command::new("ssh-keygen")
            .args(["-q", "-N", "", "-t", "ed25519", "-f"])
            .arg(&user_key)
            .status()
            .ok()?
            .success()
        {
            return None;
        }
        let user_pub = dir.join("user_key.pub");
        // OBS: `always:forever` (u64::MAX-sentinel) avvisas av `ssh-key`-
        // kratet ("invalid time" — det representerar bara giltiga
        // tidsstämplar upp till `i64::MAX` sekunder). "-5m:+1h" räcker
        // gott och gällt för ett test som körs på sekunder, och undviker
        // klockskevhet mot `-5m`.
        if !std::process::Command::new("ssh-keygen")
            .arg("-s")
            .arg(&ca_key)
            .args(["-I", "bastion-test-cert", "-n", principal, "-V", "-5m:+1h"])
            .arg(&user_pub)
            .status()
            .ok()?
            .success()
        {
            return None;
        }
        Some((dir.join("ca_key.pub"), user_key, dir.join("user_key-cert.pub")))
    }

    /// GENUIN certifikatautentisering mot en RIKTIG sshd (inte en offline-
    /// verifiering av att erbjudandet byggs rätt, som Swift-sidans
    /// `OpenSSHCertificateAuthTests` var tvungna att nöja sig med — se
    /// `ROADMAP.md`s notering om att swift-nio-ssh SERVER-rollen inte kan ta
    /// emot cert-auth alls. Ett riktigt `sshd` hanterar det fullt ut, så
    /// hela vägen — signera certifikatet, erbjud det, sshd verifierar CA +
    /// principal — bevisas här, som ett permanent CI-test.
    #[tokio::test]
    async fn certificate_auth_succeeds_with_a_valid_cert_and_trusted_ca() {
        let dir = std::env::temp_dir().join(format!("bastion-cert-ok-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let user = whoami_user();
        let Some((ca_pub, key_path, cert_path)) = make_ca_and_signed_cert(&dir, &user) else {
            eprintln!("hoppar: ssh-keygen ej tillgängligt i den här miljön");
            return;
        };
        let Some(sshd) = TestCertSshd::start(&ca_pub) else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };

        let mut host = Host::new("cert-ok".into(), "127.0.0.1".into(), user);
        host.port = sshd.port as i64;
        host.auth = HostAuth::CertificateFile {
            key_path: key_path.to_string_lossy().into_owned(),
            cert_path: cert_path.to_string_lossy().into_owned(),
        };

        let output = run_command(host, None, "echo bastion-cert-auth-ok".to_string(), None)
            .recv_blocking()
            .expect("kanalen stängdes utan svar")
            .expect("certifikatautentiseringen skulle ha lyckats mot en betrodd CA + rätt principal");
        assert_eq!(output.trim(), "bastion-cert-auth-ok");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Ett certifikat vars principal INTE matchar inloggningsanvändaren ska
    /// avvisas, trots att CA:n i sig är betrodd — sshd matchar principal
    /// mot den efterfrågade användaren (ingen `AuthorizedPrincipalsFile`
    /// konfigurerad här, så standardbeteendet gäller).
    #[tokio::test]
    async fn certificate_auth_fails_with_a_wrong_principal() {
        let dir = std::env::temp_dir().join(format!("bastion-cert-wrongp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let user = whoami_user();
        let Some((ca_pub, key_path, cert_path)) =
            make_ca_and_signed_cert(&dir, "nagon-annan-anvandare")
        else {
            eprintln!("hoppar: ssh-keygen ej tillgängligt i den här miljön");
            return;
        };
        let Some(sshd) = TestCertSshd::start(&ca_pub) else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };

        let mut host = Host::new("cert-wrong-principal".into(), "127.0.0.1".into(), user);
        host.port = sshd.port as i64;
        host.auth = HostAuth::CertificateFile {
            key_path: key_path.to_string_lossy().into_owned(),
            cert_path: cert_path.to_string_lossy().into_owned(),
        };

        let err = run_command(host, None, "echo ska-aldrig-koras".to_string(), None)
            .recv_blocking()
            .expect("kanalen stängdes utan svar")
            .expect_err("certifikat med fel principal ska avvisas, inte accepteras");
        assert!(
            err.contains("misslyckades") || err.contains("avvisade"),
            "felet ska vara tydligt, fick: {err}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Ett certifikat signerat av en CA sshd INTE litar på ska avvisas,
    /// trots giltig principal — annars vore `TrustedUserCAKeys` verkningslös.
    #[tokio::test]
    async fn certificate_auth_fails_with_an_untrusted_ca() {
        let dir = std::env::temp_dir().join(format!("bastion-cert-untrusted-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let user = whoami_user();
        // Certifikatet signeras med en ANNAN CA än den sshd konfigureras
        // att lita på (nedan) — bara `trusted_dir`s ca_key.pub hamnar i
        // `TrustedUserCAKeys`.
        let untrusted_dir = dir.join("untrusted-ca");
        std::fs::create_dir_all(&untrusted_dir).unwrap();
        let Some((_untrusted_ca_pub, key_path, cert_path)) =
            make_ca_and_signed_cert(&untrusted_dir, &user)
        else {
            eprintln!("hoppar: ssh-keygen ej tillgängligt i den här miljön");
            return;
        };
        let trusted_dir = dir.join("trusted-ca");
        std::fs::create_dir_all(&trusted_dir).unwrap();
        let Some((trusted_ca_pub, _unused_key, _unused_cert)) =
            make_ca_and_signed_cert(&trusted_dir, &user)
        else {
            eprintln!("hoppar: ssh-keygen ej tillgängligt i den här miljön");
            return;
        };
        let Some(sshd) = TestCertSshd::start(&trusted_ca_pub) else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };

        let mut host = Host::new("cert-untrusted-ca".into(), "127.0.0.1".into(), user);
        host.port = sshd.port as i64;
        host.auth = HostAuth::CertificateFile {
            key_path: key_path.to_string_lossy().into_owned(),
            cert_path: cert_path.to_string_lossy().into_owned(),
        };

        let err = run_command(host, None, "echo ska-aldrig-koras".to_string(), None)
            .recv_blocking()
            .expect("kanalen stängdes utan svar")
            .expect_err("certifikat från en obetrodd CA ska avvisas, inte accepteras");
        assert!(
            err.contains("misslyckades") || err.contains("avvisade"),
            "felet ska vara tydligt, fick: {err}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
