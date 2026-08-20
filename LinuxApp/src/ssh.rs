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
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyBase64, load_secret_key};
use russh::{Channel, ChannelMsg};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;

#[derive(Debug)]
pub enum SshEvent {
    Connected,
    Data(Vec<u8>),
    Error(String),
    /// Transporten dog under en session som var i gång — skilt från
    /// `Closed`, som bara betyder "den här sessionen är slut" oavsett
    /// anledning. Skillnaden syns för användaren: ett rent `exit` ska inte
    /// säga något alls, medan en anslutning som försvinner är precis vad
    /// man behöver få veta (annars stängs terminalen utan förklaring och
    /// skrollbufferten följer med).
    Disconnected(String),
    Closed,
}

/// Hur en shell-session tog slut. Utbruten som en egen funktion i stället
/// för tre `if`-satser inne i `run()` för att den ska gå att testa: det är
/// KLASSIFICERINGEN som är lätt att få fel, inte select-loopen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    /// Fjärrshellen avslutade sig själv (`exit`, `logout`, en dödad
    /// process). Normalt, inget att rapportera.
    RemoteExited,
    /// UI-sidan stängde rutan/fliken. Också normalt.
    ClosedLocally,
    /// Varken det ena eller det andra — kanalen tog slut utan att
    /// fjärrsidan sa varför, eller så gick en skrivning inte igenom.
    /// Det här är fallet som saknade all hantering: anslutningen var
    /// borta och ingen fick veta det.
    ConnectionLost,
}

/// Lokal stängning väger tyngst: stänger användaren fliken spelar det
/// ingen roll vad fjärrsidan hann säga, det är fortfarande ett avsiktligt
/// avslut och inget att larma om.
pub fn classify_session_end(saw_exit_status: bool, closed_locally: bool) -> SessionEnd {
    if closed_locally {
        SessionEnd::ClosedLocally
    } else if saw_exit_status {
        SessionEnd::RemoteExited
    } else {
        SessionEnd::ConnectionLost
    }
}

/// Texten användaren ser när anslutningen dör. Formulerad efter det
/// `cubic` påpekade på PR #199: en ny TCP-anslutning kan INTE ersätta den
/// gamla transparent — fjärrprocessen och dess tillstånd är borta — så
/// texten lovar inte att något återupptas, den säger att sessionen är
/// slut och att en ny måste öppnas.
pub fn connection_lost_message() -> String {
    format!(
        "Anslutningen bröts oväntat — servern slutade svara eller nätet försvann. \
         Den shell som kördes är borta och går inte att återuppta där den slutade; \
         en återanslutning startar en ny session. \
         (Upptäckt efter {} sekunder utan svar.)",
        KEEPALIVE_INTERVAL.as_secs() * (KEEPALIVE_MAX as u64 + 1)
    )
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

    /// Serverns sida av agent-vidarebefordran.
    ///
    /// När fjärrvärden vill använda vår agent öppnar den en kanal av typen
    /// `auth-agent@openssh.com` mot oss. Vi accepterar den och kopplar
    /// ihop den med den LOKALA agentens unix-socket — bytes in, bytes ut,
    /// utan att tolka agentprotokollet. Det är precis vad OpenSSH gör, och
    /// det är hela anledningen till att nycklarna aldrig lämnar maskinen:
    /// bara signaturförfrågningar färdas över kanalen.
    ///
    /// Motsvarande stöd saknas på Swift-sidan — `NIOSSH` exponerar ingen
    /// väg att ta emot en serveröppnad kanal av godtycklig typ, vilket
    /// ROADMAP dokumenterar som arkitektoniskt blockerat. Den slutsatsen
    /// gäller NIOSSH, inte russh, och därför finns funktionen här.
    ///
    /// Saknas `$SSH_AUTH_SOCK` accepteras kanalen ändå och stängs direkt.
    /// Alternativet vore att avvisa den, men då får fjärrsidan ett
    /// protokollfel i stället för ett tomt svar — och en `ssh-add -l` som
    /// säger "inga identiteter" är begripligare än en bruten session.
    async fn server_channel_open_agent_forward(
        &mut self,
        channel: Channel<Msg>,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        let socket = std::env::var("SSH_AUTH_SOCK").ok();
        tokio::spawn(async move {
            let Some(socket) = socket else {
                return; // ingen agent lokalt — kanalen stängs när den droppas
            };
            let Ok(mut local) = tokio::net::UnixStream::connect(&socket).await else {
                return;
            };
            let mut remote = channel.into_stream();
            // Samma brygga som port_forward och socks_proxy använder.
            // Agentprotokollet är längdprefixade meddelanden i båda
            // riktningar, så en rå kopiering räcker — inget behöver
            // ramas om på vägen.
            let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
        });
        Ok(())
    }

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

/// Hur ofta en TYST anslutning ska pinga servern. russh skickar en
/// SSH-keepalive först när ingenting hörts på så här länge, så en session
/// där det faktiskt flödar data betalar ingenting för det här.
///
/// 30 sekunder ligger under de idle-timeouts som är vanliga i NAT-tabeller
/// och hos brandväggar (ofta 60 s eller mer) — hela poängen är att en
/// session man lämnar öppen i en annan flik inte ska vara tyst död när man
/// kommer tillbaka.
const KEEPALIVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// Hur många obesvarade keepalives som får gå innan anslutningen förklaras
/// död. Det här är DÖD-DETEKTERINGEN, som saknades helt: utan den märker
/// klienten aldrig att en server försvann utan att stänga TCP-anslutningen
/// rent (strömavbrott, tappat nät, en brandvägg som glömde flödet) — den
/// bara sitter och väntar för alltid.
///
/// 3 × 30 s ≈ två minuter innan en död anslutning rapporteras. Lägre vore
/// snabbare men skulle döda sessioner i onödan på riktigt dåliga nät.
const KEEPALIVE_MAX: usize = 3;

/// russh har både keepalive och död-detektering inbyggt, men AVSTÄNGT som
/// standard (`keepalive_interval: None`). Den här funktionen finns för att
/// ingen anslutningsväg ska kunna glömma att slå på dem — direkt, genom
/// jump-host, engångskommandon och portvidarebefordran delar alla på den.
fn client_config() -> Arc<client::Config> {
    Arc::new(client::Config {
        keepalive_interval: Some(KEEPALIVE_INTERVAL),
        keepalive_max: KEEPALIVE_MAX,
        ..client::Config::default()
    })
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
    let config = client_config();

    // Genom en SOCKS5-proxy när värden har en. Transporten byts ut, INTE
    // SSH-lagret: `connect_stream` kör exakt samma handskakning och
    // värdnyckelkontroll som `connect`, bara ovanpå en ström vi redan
    // öppnat. Samma mönster som jump-vägen redan använder.
    //
    // Notera att MÅLETS namn skickas vidare till proxyn ouppslaget — det är
    // hela poängen med en tailnet-proxy, där namnet bara betyder något i
    // andra änden.
    if let Some(proxy) = host.socks_proxy.as_deref().filter(|p| !p.is_empty()) {
        let stream = tokio::time::timeout(
            CONNECT_TIMEOUT,
            crate::socks_proxy::connect_via_socks5(proxy, &host.host_name, host.port as u16),
        )
        .await
        .map_err(|_| format!("SOCKS5-proxyn {proxy} svarade inte inom {}s", CONNECT_TIMEOUT.as_secs()))?
        .map_err(|e| format!("kunde inte nå {} genom proxyn: {e}", host.host_name))?;

        return tokio::time::timeout(
            CONNECT_TIMEOUT,
            client::connect_stream(config, stream, handler),
        )
        .await
        .map_err(|_| {
            format!(
                "SSH-handskakningen genom proxyn svarade inte inom {}s",
                CONNECT_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("anslutning genom proxyn misslyckades: {e}"));
    }

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

    let config = client_config();
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
    tokio::time::timeout(
        COMMAND_TIMEOUT,
        run_command_on_session(&session, &command, host.forward_agent),
    )
    .await
    .map_err(|_| format!("kommandot svarade inte inom {}s", COMMAND_TIMEOUT.as_secs()))?
}

async fn run_command_on_session(
    session: &Handle<ClientHandler>,
    command: &str,
    forward_agent: bool,
) -> Result<String, String> {
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("kunde inte öppna kanal: {e}"))?;
    // Även ett engångskommando kan behöva agenten — `git pull` eller
    // `ssh vidare-värd` på fjärrsidan använder den precis som en
    // interaktiv session gör. Begäran måste ligga före `exec`, av samma
    // skäl som före `request_shell`: servern sätter $SSH_AUTH_SOCK när
    // kommandot startar.
    if forward_agent {
        channel
            .agent_forward(false)
            .await
            .map_err(|e| format!("kunde inte begära agent-vidarebefordran: {e}"))?;
    }
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
    // Agent-vidarebefordran begärs FÖRE pty och shell, precis som
    // OpenSSH gör: begäran gäller kanalen, och servern sätter
    // $SSH_AUTH_SOCK i miljön när shellen startar. Efteråt vore för sent.
    //
    // Ett nekat svar är inte ett fel som ska avbryta anslutningen —
    // många servrar har `AllowAgentForwarding no`, och då ska sessionen
    // öppnas ändå, bara utan agenten. Därför `false` (want_reply) och
    // inget felkast: vi ber, och accepterar att svaret kan bli nej.
    if host.forward_agent {
        channel
            .agent_forward(false)
            .await
            .map_err(|e| format!("kunde inte begära agent-vidarebefordran: {e}"))?;
    }

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

    // Två flaggor, inte en: `break` ensamt säger bara ATT loopen tog slut,
    // aldrig varför — och det är just varför som avgör om användaren ska
    // se något. Se `classify_session_end`.
    let mut saw_exit_status = false;
    let mut closed_locally = false;
    loop {
        tokio::select! {
            incoming = input_rx.recv() => {
                match incoming {
                    Ok(bytes) => {
                        // En skrivning som inte går igenom betyder att
                        // transporten är borta — INTE ett rent avslut.
                        if channel.data(&bytes[..]).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        closed_locally = true; // UI-sidan stängde input-kanalen
                        break;
                    }
                }
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        if output_tx.send(SshEvent::Data(data.to_vec())).await.is_err() {
                            closed_locally = true; // ingen lyssnar längre
                            break;
                        }
                    }
                    Some(ChannelMsg::ExitStatus { .. }) => {
                        saw_exit_status = true;
                        break;
                    }
                    // Kanalen tog slut utan att fjärrsidan sa varför. Det
                    // är så en död anslutning ser ut härifrån — russh
                    // river sessionen när keepalives slutar besvaras.
                    None => break,
                    _ => {}
                }
            }
        }
    }

    if classify_session_end(saw_exit_status, closed_locally) == SessionEnd::ConnectionLost {
        let _ = output_tx
            .send(SshEvent::Disconnected(connection_lost_message()))
            .await;
    }
    Ok(())
}

/// Sida som förklarar varför RSA är avstängt. Visas som klickbar länk i
/// dialogen (`main.rs`) och i klartext i terminalens felrad.
pub const RSA_DISABLED_DOC_URL: &str =
    "https://github.com/blixten85/bastion/blob/main/docs/RSA-INAKTIVERAT.md";

/// Inledningen på felmeddelandet. `main.rs` känner igen felet på den här
/// strängen och byter ut terminalraden mot en dialog med klickbar länk.
pub const RSA_DISABLED_PREFIX: &str = "RSA-nycklar är tillfälligt inaktiverade";

/// Felet som varje RSA-väg returnerar. Formuleras på ett ställe så att
/// nyckelfil, certifikat och ssh-agent säger exakt samma sak.
/// Vad agent-autentiseringen faktiskt stötte på, när ingen nyckel gick
/// igenom.
///
/// Ren funktion och egen typ av ett skäl: den gamla koden avgjorde det
/// här i en villkorskedja mitt i en async-loop, där det varken gick att
/// testa eller att utöka utan att läsa hela loopen. Nu är beslutet en
/// tabell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AgentAttempt {
    /// Nycklar som faktiskt provades mot servern.
    pub considered: usize,
    /// RSA-nycklar som hoppades över (RUSTSEC-2023-0071).
    pub skipped_rsa: usize,
    /// Säkerhetsnycklar (FIDO2/YubiKey, `sk-ssh-ed25519` och
    /// `sk-ecdsa-sha2-nistp256`) bland de provade.
    pub security_keys: usize,
}

/// Felmeddelandet när ingen identitet i agenten dög.
///
/// Tre olika situationer som ser likadana ut för användaren men kräver
/// helt olika saker:
///
/// 1. Bara RSA fanns — stödet är avstängt, byt nyckeltyp.
/// 2. En säkerhetsnyckel provades — den kräver en FYSISK BERÖRING, och
///    utan den timeoutar signeringen. Det ser ut som att servern nekade,
///    men ingen har nekat något: token väntade på ett finger.
/// 3. Vanliga nycklar provades och servern sa nej — det är ett riktigt
///    auth-fel.
///
/// Fall 2 är det som annars är omöjligt att gissa sig till. En YubiKey
/// blinkar tyst i en USB-port, ofta bakom datorn.
pub fn agent_failure_message(attempt: AgentAttempt) -> String {
    if attempt.considered == 0 && attempt.skipped_rsa > 0 {
        return rsa_disabled_error();
    }
    if attempt.security_keys > 0 {
        let plural = if attempt.security_keys == 1 { "en säkerhetsnyckel" } else { "säkerhetsnycklar" };
        return format!(
            "servern avvisade autentiseringen. Agenten hade {plural} (FIDO2/YubiKey),              och en sådan måste RÖRAS VID för att signera — sitter den i en port du              inte ser kan den ha väntat på ett finger tills försöket gav upp.              Kontrollera att den blinkar och försök igen."
        );
    }
    "servern avvisade autentiseringen".to_string()
}

fn rsa_disabled_error() -> String {
    format!(
        "{RSA_DISABLED_PREFIX}. Stödet är avstängt tills RUSTSEC-2023-0071 \
         (Marvin-attacken) i crate:n rsa har en rättad version. Använd en \
         Ed25519-nyckel under tiden. Läs mer: {RSA_DISABLED_DOC_URL}"
    )
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
            if key.algorithm().is_rsa() {
                return Err(rsa_disabled_error());
            }
            // Hash-valet gällde bara RSA (ssh-rsa/rsa-sha2-256/-512). Så
            // länge RSA är avstängt är `None` det enda korrekta värdet —
            // för Ed25519/ECDSA ignorerades det ändå. Den tidigare
            // `best_supported_rsa_hash`-förhandlingen finns inte att
            // anropa utan russh:s `rsa`-feature.
            session
                .authenticate_publickey(&host.user, PrivateKeyWithHashAlg::new(Arc::new(key), None))
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
            // Räknar RSA-identiteter som hoppas över, så att en agent som
            // BARA har RSA-nycklar får förklaringen nedan i stället för
            // det intetsägande "servern avvisade autentiseringen".
            let mut skipped_rsa = 0usize;
            let mut considered = 0usize;
            let mut security_keys = 0usize;
            for identity in identities {
                // `request_identities` ger nu `AgentIdentity` (nyckel +
                // kommentar, eller ett certifikat). Bara rena publika
                // nycklar används här — certifikat via agent har en egen
                // väg (`HostAuth::CertificateFile`).
                let russh::keys::agent::AgentIdentity::PublicKey { key, .. } = identity else {
                    continue;
                };
                // RSA hoppas över helt så länge RUSTSEC-2023-0071 är
                // olöst — tidigare valdes `HashAlg::Sha256` här.
                if key.algorithm().is_rsa() {
                    skipped_rsa += 1;
                    continue;
                }
                considered += 1;
                // `sk-`-algoritmerna är FIDO2/säkerhetsnycklar. russh
                // stöder dem (`SkEd25519`, `SkEcdsaSha2NistP256`), och de
                // går genom agenten precis som andra nycklar — men de
                // kräver en fysisk beröring, vilket ingen annan nyckeltyp
                // gör. Räknas för att felmeddelandet ska kunna säga det.
                if matches!(
                    key.algorithm(),
                    russh::keys::Algorithm::SkEd25519 | russh::keys::Algorithm::SkEcdsaSha2NistP256
                ) {
                    security_keys += 1;
                }
                let result = session
                    .authenticate_publickey_with(&host.user, key, None, &mut agent)
                    .await;
                if matches!(result, Ok(ref r) if r.success()) {
                    succeeded = true;
                    break;
                }
            }
            if !succeeded {
                return Err(agent_failure_message(AgentAttempt {
                    considered,
                    skipped_rsa,
                    security_keys,
                }));
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
            // Certifikatet signeras av CA:n, men själva autentiseringen
            // signeras med användarnyckeln — är den RSA gäller samma stopp.
            if key.algorithm().is_rsa() {
                return Err(rsa_disabled_error());
            }
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

    /// Tre situationer som ser likadana ut för användaren men kräver
    /// helt olika saker. Den mellersta är den som annars är omöjlig att
    /// gissa sig till: en YubiKey blinkar tyst i en port bakom datorn.
    #[test]
    fn agent_failure_tells_the_three_situations_apart() {
        // Bara RSA fanns — inget provades.
        let only_rsa = agent_failure_message(AgentAttempt {
            considered: 0,
            skipped_rsa: 2,
            security_keys: 0,
        });
        assert!(only_rsa.starts_with(RSA_DISABLED_PREFIX), "{only_rsa}");

        // En säkerhetsnyckel provades — kan ha väntat på en beröring.
        let sk = agent_failure_message(AgentAttempt {
            considered: 1,
            skipped_rsa: 0,
            security_keys: 1,
        });
        assert!(sk.contains("RÖRAS VID"), "{sk}");
        assert!(sk.contains("en säkerhetsnyckel"), "singular när det är en: {sk}");

        let many = agent_failure_message(AgentAttempt {
            considered: 3,
            skipped_rsa: 0,
            security_keys: 2,
        });
        assert!(many.contains("säkerhetsnycklar"), "plural när det är flera: {many}");

        // Vanliga nycklar, servern sa nej. Inget mer att tillägga.
        let plain = agent_failure_message(AgentAttempt {
            considered: 2,
            skipped_rsa: 0,
            security_keys: 0,
        });
        assert_eq!(plain, "servern avvisade autentiseringen");
        assert!(!plain.contains(RSA_DISABLED_PREFIX));
    }

    /// RSA-fallet gäller bara när INGET annat provades. Fanns det en
    /// användbar nyckel också är det servern som nekat, inte
    /// RSA-avstängningen — och då hade RSA-texten pekat användaren åt
    /// fel håll.
    #[test]
    fn rsa_message_only_when_nothing_else_was_tried() {
        let mixed = agent_failure_message(AgentAttempt {
            considered: 1,
            skipped_rsa: 3,
            security_keys: 0,
        });
        assert!(!mixed.starts_with(RSA_DISABLED_PREFIX), "{mixed}");
        assert_eq!(mixed, "servern avvisade autentiseringen");
    }

    /// `main.rs` känner igen RSA-felet på `RSA_DISABLED_PREFIX` för att
    /// kunna visa dialogen med den klickbara länken. Formuleras meddelandet
    /// om utan att prefixet står först försvinner dialogen tyst och
    /// användaren får bara en röd terminalrad — det här testet fångar det.
    #[test]
    fn rsa_disabled_error_matches_the_prefix_main_rs_dispatches_on() {
        let msg = rsa_disabled_error();
        assert!(
            msg.starts_with(RSA_DISABLED_PREFIX),
            "felmeddelandet måste börja med prefixet, var: {msg}"
        );
        assert!(
            msg.contains(RSA_DISABLED_DOC_URL),
            "URL:en ska med i klartext för terminalraden, var: {msg}"
        );
    }

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
                Ok(SshEvent::Disconnected(msg)) => return Err(msg),
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
        let user = crate::test_support::test_user();
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
        let user = crate::test_support::test_user();
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
        let user = crate::test_support::test_user();
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
        let user = crate::test_support::test_user();
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
        let user = crate::test_support::test_user();
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

    use crate::test_support::test_user as whoami_user;

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
        let output = run_command_on_session(&session, "echo bastion-proxyjump-ok", false)
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

    /// Agent-vidarebefordran mot en RIKTIG sshd med en RIKTIG ssh-agent.
    ///
    /// Testet finns för att ROADMAP dokumenterar agent forwarding som
    /// "arkitektoniskt blockerad". Det gäller NIOSSH på Swift-sidan, som
    /// inte exponerar någon väg att ta emot en serveröppnad kanal av
    /// godtycklig typ. russh har både `Channel::agent_forward` och
    /// `Handler::server_channel_open_agent_forward`, och det här visar att
    /// de faktiskt räcker hela vägen.
    ///
    /// Beviset är `ssh-add -l` PÅ FJÄRRSIDAN: den listar nycklarna ur den
    /// LOKALA agenten, vilket bara är möjligt om kanalen kopplats ihop med
    /// vår unix-socket. Att bara kontrollera att `$SSH_AUTH_SOCK` är satt
    /// hade räckt för att servern accepterade begäran — inte för att
    /// bryggan fungerar.
    #[test]
    #[ignore = "kräver en riktig localhost-sshd, en nyckel i authorized_keys och en körande ssh-agent"]
    fn forwarded_agent_is_reachable_from_the_remote_side() {
        let key_path = std::env::var("BASTION_TEST_SSH_KEY").expect("BASTION_TEST_SSH_KEY måste sättas");
        assert!(
            std::env::var("SSH_AUTH_SOCK").is_ok(),
            "testet kräver en körande ssh-agent (SSH_AUTH_SOCK)"
        );

        let mut host = Host::new("agent-forward-test".into(), "127.0.0.1".into(), crate::test_support::test_user());
        host.auth = HostAuth::KeyFile(key_path);
        host.forward_agent = true;

        let rx = run_command(host, None, "ssh-add -l 2>&1".to_string(), None);
        let output = rx
            .recv_blocking()
            .expect("kanalen stängdes utan svar")
            .expect("kommandot misslyckades");

        // Nycklarna kommer från VÅR agent. Hade bryggan inte fungerat
        // skulle `ssh-add` svara "Could not open a connection to your
        // authentication agent" eller "The agent has no identities".
        assert!(
            output.contains("SHA256:"),
            "fjärrsidan såg inga nycklar genom den vidarebefordrade agenten, fick: {output:?}"
        );
    }


    /// En TCP-relä som går att SVARTHÅLA: den slutar flytta bytes åt båda
    /// håll men håller båda socketarna ÖPPNA. Det är precis vad ett tappat
    /// nät gör, och exakt det fall TCP inte upptäcker av sig självt — utan
    /// keepalive sitter klienten och väntar för alltid. Att i stället
    /// stänga socketarna vore ett helt annat (och redan hanterat) fall:
    /// då kommer ett RST/FIN och russh märker det direkt.
    ///
    /// Returnerar porten reläet lyssnar på plus flaggan som svarthålar den.
    fn spawn_blackhole_relay(target_port: u16) -> Option<(u16, Arc<std::sync::atomic::AtomicBool>)> {
        use std::io::{Read, Write};
        use std::sync::atomic::{AtomicBool, Ordering};

        let port = crate::test_support::reserve_port()?;
        let listener = std::net::TcpListener::bind(("127.0.0.1", port)).ok()?;
        let blackhole = Arc::new(AtomicBool::new(false));

        let flag = blackhole.clone();
        std::thread::spawn(move || {
            let Ok((client, _)) = listener.accept() else {
                return;
            };
            let Ok(server) = std::net::TcpStream::connect(("127.0.0.1", target_port)) else {
                return;
            };
            // Kort lästimeout så pumparna hinner reagera på flaggan i
            // stället för att blockera i en read som aldrig återkommer.
            let _ = client.set_read_timeout(Some(Duration::from_millis(100)));
            let _ = server.set_read_timeout(Some(Duration::from_millis(100)));

            let pairs = [
                (client.try_clone(), server.try_clone()),
                (server.try_clone(), client.try_clone()),
            ];
            let mut handles = Vec::new();
            for (src, dst) in pairs {
                let (Ok(mut src), Ok(mut dst)) = (src, dst) else {
                    return;
                };
                let flag = flag.clone();
                handles.push(std::thread::spawn(move || {
                    let deadline = std::time::Instant::now() + Duration::from_secs(120);
                    let mut buf = [0u8; 8192];
                    while std::time::Instant::now() < deadline {
                        if flag.load(Ordering::Relaxed) {
                            // Håll socketarna vid liv utan att flytta något.
                            // Att returnera här skulle droppa dem, alltså
                            // stänga anslutningen rent — motsatsen till målet.
                            std::thread::sleep(Duration::from_millis(50));
                            continue;
                        }
                        match src.read(&mut buf) {
                            Ok(0) => return,
                            Ok(n) => {
                                if dst.write_all(&buf[..n]).is_err() {
                                    return;
                                }
                            }
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                                ) => {}
                            Err(_) => return,
                        }
                    }
                }));
            }
            for handle in handles {
                let _ = handle.join();
            }
        });

        Some((port, blackhole))
    }

    /// Bevisar att keepalive-konfigurationen faktiskt UPPTÄCKER en död
    /// anslutning — inte bara att fälten är satta.
    ///
    /// Uppställningen är en riktig sshd bakom en relä som svarthålas mitt
    /// i en levande session. Kontrollen i samma test är själva poängen:
    /// EXAKT samma svarthålning med `Config::default()` (russh:s standard,
    /// alltså keepalive AVSTÄNGT) lämnar klienten hängande. Utan den
    /// jämförelsen skulle testet inte kunna skilja "keepalive upptäckte
    /// det" från "något annat rev anslutningen ändå".
    ///
    /// Snabba konstanter (1 s × 1) i stället för produktionens 30 s × 3 —
    /// samma mekanism i russh, bara en tidsskala som ryms i en testsvit.
    #[tokio::test]
    async fn a_blackholed_connection_is_detected_with_keepalive_and_not_without() {
        let Some(sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };

        /// Ansluter genom en egen relä, svarthålar den och svarar på om
        /// anslutningen dog inom fönstret.
        async fn dies_within(
            sshd: &TestSshd,
            keepalive: Option<Duration>,
            window: Duration,
        ) -> Option<bool> {
            let (relay_port, blackhole) = spawn_blackhole_relay(sshd.port)?;
            let mut host = host_for(sshd, "keepalive");
            host.port = relay_port as i64;

            let config = Arc::new(client::Config {
                keepalive_interval: keepalive,
                keepalive_max: 1,
                ..client::Config::default()
            });
            let handler = ClientHandler {
                host: host.host_name.clone(),
                port: relay_port,
                known_hosts: Arc::new(
                    KnownHosts::open(Some(crate::test_support::known_hosts_path())).ok()?,
                ),
                remote_forwards: RemoteForwards::default(),
            };
            let mut session =
                client::connect(config, ("127.0.0.1", relay_port), handler).await.ok()?;
            authenticate(&mut session, &host, None).await.ok()?;

            // Sessionen ska bevisligen LEVA innan vi river nätet under den,
            // annars mäter testet bara en anslutning som aldrig kom upp.
            let alive = run_command_on_session(&session, "echo levande", false).await.ok()?;
            assert_eq!(alive.trim(), "levande");

            let mut channel = session.channel_open_session().await.ok()?;
            channel.request_pty(false, "xterm-256color", 80, 24, 0, 0, &[]).await.ok()?;
            channel.request_shell(true).await.ok()?;

            blackhole.store(true, std::sync::atomic::Ordering::Relaxed);

            // Kanalen tar slut när russh river sessionen. Timeout = den
            // överlevde fönstret.
            Some(
                tokio::time::timeout(window, async {
                    while channel.wait().await.is_some() {}
                })
                .await
                .is_ok(),
            )
        }

        let window = Duration::from_secs(15);
        let Some(with_keepalive) = dies_within(&sshd, Some(Duration::from_secs(1)), window).await
        else {
            eprintln!("hoppar: kunde inte sätta upp reläet i den här miljön");
            return;
        };
        assert!(
            with_keepalive,
            "med keepalive PÅ ska en svarthålad anslutning upptäckas — \
             det är hela död-detekteringen"
        );

        let Some(without_keepalive) = dies_within(&sshd, None, Duration::from_secs(6)).await else {
            eprintln!("hoppar: kunde inte sätta upp reläet i den här miljön");
            return;
        };
        assert!(
            !without_keepalive,
            "utan keepalive ska samma svarthålning INTE märkas — gör den det \
             är det något annat än keepalive som river anslutningen, och då \
             bevisar testet ovan ingenting"
        );
    }

    /// Kärnan i död-detekteringen. Att kanalen tar slut säger ingenting i
    /// sig — det gör den vid ett rent `exit` också. Det som skiljer är om
    /// fjärrsidan hann säga varför, och om det var vi själva som stängde.
    /// Får den här klassificeringen fel svar syns det på två sätt, båda
    /// illa: antingen larmar appen om en död anslutning varje gång någon
    /// skriver `exit`, eller så försvinner en riktig anslutningsförlust
    /// tyst tillsammans med terminalfönstret.
    #[test]
    fn only_an_end_without_explanation_counts_as_a_lost_connection() {
        assert_eq!(
            classify_session_end(false, false),
            SessionEnd::ConnectionLost,
            "kanalen tog slut utan exit-status och utan att vi stängde — anslutningen dog"
        );
        assert_eq!(
            classify_session_end(true, false),
            SessionEnd::RemoteExited,
            "fjärrsidan skickade exit-status, alltså ett rent avslut"
        );
        assert_eq!(
            classify_session_end(false, true),
            SessionEnd::ClosedLocally,
            "vi stängde rutan själva"
        );
    }

    /// Ett kapplöpningsfall som faktiskt inträffar: användaren stänger
    /// fliken i samma ögonblick som fjärrshellen avslutas, så BÅDA
    /// flaggorna är satta. Lokal stängning måste väga tyngst — annars
    /// skulle en avsiktligt stängd flik kunna rapporteras som något
    /// användaren behöver agera på.
    #[test]
    fn closing_locally_wins_over_whatever_the_remote_side_said() {
        assert_eq!(classify_session_end(true, true), SessionEnd::ClosedLocally);
    }

    /// Meddelandet ska inte lova att sessionen återupptas — den kan inte
    /// det (`cubic`-fyndet på PR #199: fjärrprocessens tillstånd är borta
    /// när transporten faller). Och tiden som nämns måste följa av de
    /// faktiska keepalive-konstanterna, inte vara en siffra som står kvar
    /// när någon justerar dem.
    #[test]
    fn the_disconnect_message_states_a_real_timeout_and_promises_no_resume() {
        let msg = connection_lost_message();
        let expected_seconds = KEEPALIVE_INTERVAL.as_secs() * (KEEPALIVE_MAX as u64 + 1);
        assert!(
            msg.contains(&expected_seconds.to_string()),
            "tiden ska härledas ur konstanterna, fick: {msg}"
        );
        assert!(msg.contains("går inte att återuppta"), "fick: {msg}");
    }

    /// Hela poängen med `client_config`: russh har keepalive OCH
    /// död-detektering inbyggt men AVSTÄNGT som standard. Faller den här
    /// tillbaka på `Config::default()` är båda borta igen utan att något
    /// annat test märker det.
    #[test]
    fn the_shared_client_config_actually_turns_keepalive_on() {
        let config = client_config();
        assert_eq!(config.keepalive_interval, Some(KEEPALIVE_INTERVAL));
        assert_eq!(config.keepalive_max, KEEPALIVE_MAX);
        assert!(
            client::Config::default().keepalive_interval.is_none(),
            "om russh någon gång slår på det här som standard är den här funktionen \
             överflödig — men just nu är den inte det, och det är varför den finns"
        );
    }

    /// Hela vägen: en värd med `socks_proxy` satt ska nå en RIKTIG sshd
    /// GENOM en riktig SOCKS5-proxy — och den proxyn är Bastions egen,
    /// tunnlad över en annan riktig sshd.
    ///
    /// Bevisar det som inte går att se på delarna var för sig: att
    /// `connect_stream` gör samma handskakning och värdnyckelkontroll
    /// ovanpå en ström vi öppnat själva som `connect` gör på en den öppnar.
    #[tokio::test]
    async fn a_host_with_a_socks_proxy_reaches_a_real_sshd_through_it() {
        let Some(proxy_sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let Some(target_sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };

        // Bastions egen SOCKS5-proxy, tunnlad genom den första sshd:n.
        let proxy_host = host_for(&proxy_sshd, "proxy");
        let rx = crate::socks_proxy::spawn_dynamic_forward(
            proxy_host, None, "127.0.0.1".into(), 0, None,
        );
        let forward = rx.recv().await.unwrap().expect("proxyn startade inte");

        // Målvärden når vi BARA genom proxyn, enligt konfigurationen.
        let mut target = host_for(&target_sshd, "genom-proxy");
        target.socks_proxy = Some(format!("127.0.0.1:{}", forward.actual_bind_port));

        let session = connect(&target, None, None, None)
            .await
            .expect("anslutning genom SOCKS5-proxyn misslyckades");
        let output = run_command_on_session(&session, "echo bastion-socks-ok", false)
            .await
            .expect("kommandot över den proxyade sessionen misslyckades");
        assert_eq!(output.trim(), "bastion-socks-ok");

        forward.stop();
    }

    /// Ett tomt proxyfält ska betyda "ingen proxy", inte "anslut till
    /// adressen tomma strängen". En tom textruta i gränssnittet är hur ett
    /// bortplockat värde ser ut.
    #[tokio::test]
    async fn an_empty_proxy_field_means_no_proxy() {
        let Some(sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };
        let mut host = host_for(&sshd, "tom-proxy");
        host.socks_proxy = Some(String::new());
        assert!(
            connect(&host, None, None, None).await.is_ok(),
            "tom sträng ska behandlas som ingen proxy alls"
        );
    }
}
