//! En Telnet-anslutning (RFC 854) — rå TCP, INGEN kryptering. Port av
//! `Sources/SSHCore/Telnet.swift`. Delar inget med `ssh.rs` — helt olika
//! protokoll, bara samma "spawna på egen bakgrundstråd, prata via
//! `async_channel`"-mönster (samma skäl som `ssh.rs`: GTK:s huvudloop är
//! glib, inte tokio).
//!
//! Relevant för äldre nätverksutrustning (switchar/routrar/UPS:er/
//! seriell-över-nätverk-adaptrar) som bara exponerar Telnet, inte SSH —
//! ett protokoll Termius stöder men Bastion helt saknade innan detta.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Debug)]
pub enum TelnetEvent {
    Connected,
    Data(Vec<u8>),
    Error(String),
    Closed,
}

pub struct TelnetHandle {
    pub input: async_channel::Sender<Vec<u8>>,
    pub output: async_channel::Receiver<TelnetEvent>,
}

const IAC: u8 = 255;
const SE: u8 = 240;
const SB: u8 = 250;
const WILL: u8 = 251;
const WONT: u8 = 252;
const DO: u8 = 253;
const DONT: u8 = 254;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Data,
    SawIac,
    SawNegotiationCommand(u8),
    InSubnegotiation { saw_iac: bool },
}

/// Ren state machine för Telnets IAC-förhandling (RFC 854/855) — ingen
/// nätverkskod, enkelt att enhetstesta i isolering. Refuserar ALLTID varje
/// förhandlat alternativ (`WILL`→`DONT`, `DO`→`WONT`) istället för att
/// implementera enskilda optioner (echo, terminal type, etc): enklast
/// möjliga korrekta klientbeteende — servern faller tillbaka till "rått"
/// NVT-läge, precis vad en enkel terminalvy vill ha. Subförhandlingar
/// (`SB`...`SE`) kastas bort helt av samma skäl (vi har inget alternativ
/// att svara på).
pub struct IacFilter {
    state: State,
}

impl IacFilter {
    pub fn new() -> Self {
        IacFilter { state: State::Data }
    }

    /// Bearbetar inkommande rådata. Returnerar den rensade nyttolasten
    /// (skickas vidare till appen) och ev. svarssekvenser (skickas tillbaka
    /// till servern, i den ordning de genererades).
    pub fn process(&mut self, input: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut data = Vec::new();
        let mut replies = Vec::new();
        for &byte in input {
            self.state = match self.state {
                State::Data => {
                    if byte == IAC {
                        State::SawIac
                    } else {
                        data.push(byte);
                        State::Data
                    }
                }
                State::SawIac => match byte {
                    IAC => {
                        // IAC IAC = en literal 0xFF-databyte, inte ett kommando.
                        data.push(IAC);
                        State::Data
                    }
                    WILL | DO | WONT | DONT => State::SawNegotiationCommand(byte),
                    SB => State::InSubnegotiation { saw_iac: false },
                    // NOP/DM/BRK/IP/AO/AYT/EC/EL/GA m.fl. — inget optionsbyte,
                    // inget att svara på.
                    _ => State::Data,
                },
                State::SawNegotiationCommand(command) => {
                    match command {
                        WILL => replies.extend_from_slice(&[IAC, DONT, byte]),
                        DO => replies.extend_from_slice(&[IAC, WONT, byte]),
                        _ => {} // WONT/DONT från servern kräver inget svar.
                    }
                    State::Data
                }
                State::InSubnegotiation { saw_iac } => {
                    if saw_iac {
                        if byte == SE {
                            State::Data
                        } else {
                            State::InSubnegotiation { saw_iac: false }
                        }
                    } else if byte == IAC {
                        State::InSubnegotiation { saw_iac: true }
                    } else {
                        State::InSubnegotiation { saw_iac: false }
                    }
                }
            };
        }
        (data, replies)
    }
}

impl Default for IacFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Startar Telnet-anslutningen på en ny bakgrundstråd — samma mönster som
/// `ssh::spawn_shell`.
pub fn spawn(host: String, port: u16) -> TelnetHandle {
    let (input_tx, input_rx) = async_channel::unbounded::<Vec<u8>>();
    let (output_tx, output_rx) = async_channel::unbounded::<TelnetEvent>();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för telnet-tråden");
        rt.block_on(async move {
            if let Err(e) = run(host, port, input_rx, output_tx.clone()).await {
                let _ = output_tx.send(TelnetEvent::Error(e)).await;
            }
            let _ = output_tx.send(TelnetEvent::Closed).await;
        });
    });

    TelnetHandle {
        input: input_tx,
        output: output_rx,
    }
}

async fn run(
    host: String,
    port: u16,
    input_rx: async_channel::Receiver<Vec<u8>>,
    output_tx: async_channel::Sender<TelnetEvent>,
) -> Result<(), String> {
    let stream = TcpStream::connect((host.as_str(), port))
        .await
        .map_err(|e| format!("anslutning misslyckades: {e}"))?;
    let (mut read_half, mut write_half) = stream.into_split();
    let _ = output_tx.send(TelnetEvent::Connected).await;

    let mut filter = IacFilter::new();
    let mut buf = [0u8; 4096];
    loop {
        tokio::select! {
            incoming = input_rx.recv() => {
                match incoming {
                    Ok(bytes) => {
                        if write_half.write_all(&bytes).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break, // UI-sidan stängde input-kanalen
                }
            }
            n = read_half.read(&mut buf) => {
                match n {
                    Ok(0) => break, // EOF — fjärrsidan stängde
                    Ok(n) => {
                        let (data, replies) = filter.process(&buf[..n]);
                        if !replies.is_empty() && write_half.write_all(&replies).await.is_err() {
                            break;
                        }
                        if !data.is_empty() && output_tx.send(TelnetEvent::Data(data)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_data_passes_through_unchanged() {
        let mut filter = IacFilter::new();
        let (data, replies) = filter.process(b"hello\r\n");
        assert_eq!(data, b"hello\r\n");
        assert!(replies.is_empty());
    }

    #[test]
    fn escaped_iac_byte_is_preserved_as_literal_0xff() {
        let mut filter = IacFilter::new();
        let (data, replies) = filter.process(&[0x41, 255, 255, 0x42]);
        assert_eq!(data, vec![0x41, 255, 0x42]);
        assert!(replies.is_empty());
    }

    /// Servern erbjuder ett alternativ (WILL echo) — klienten ska refusera
    /// (DONT) istället för att implementera echo-optionen.
    #[test]
    fn will_is_refused_with_dont() {
        let mut filter = IacFilter::new();
        let (data, replies) = filter.process(&[255, 251, 1]); // IAC WILL ECHO
        assert!(data.is_empty());
        assert_eq!(replies, vec![255, 254, 1]); // IAC DONT ECHO
    }

    /// Servern begär att KLIENTEN aktiverar ett alternativ (DO) — refuseras
    /// med WONT.
    #[test]
    fn do_is_refused_with_wont() {
        let mut filter = IacFilter::new();
        let (data, replies) = filter.process(&[255, 253, 24]); // IAC DO TERMINAL-TYPE
        assert!(data.is_empty());
        assert_eq!(replies, vec![255, 252, 24]); // IAC WONT TERMINAL-TYPE
    }

    /// WONT/DONT är redan negativa svar — inget eget svar krävs.
    #[test]
    fn wont_and_dont_require_no_reply() {
        let mut filter = IacFilter::new();
        let (data, replies) = filter.process(&[255, 252, 1, 255, 254, 24]);
        assert!(data.is_empty());
        assert!(replies.is_empty());
    }

    /// Subförhandling (SB...SE) kastas bort helt — vi förhandlar inga
    /// alternativ, så det finns inget giltigt svar att extrahera ur den.
    #[test]
    fn subnegotiation_is_discarded() {
        let mut filter = IacFilter::new();
        let mut sb: Vec<u8> = vec![255, 250, 24, 0, 0x78, 0x74, 0x65, 0x72, 0x6d, 255, 240]; // IAC SB TERMINAL-TYPE IS "xterm" IAC SE
        sb.extend_from_slice(b"ok");
        let (data, replies) = filter.process(&sb);
        assert_eq!(data, b"ok");
        assert!(replies.is_empty());
    }

    /// Kommandon utan optionsbyte (t.ex. NOP) ska bara konsumeras, inte
    /// läcka in i databufferten eller tolkas som ett optionsbyte.
    #[test]
    fn command_without_option_byte_is_consumed() {
        let mut filter = IacFilter::new();
        let (data, replies) = filter.process(&[255, 241, b'x']); // IAC NOP, sen "x"
        assert_eq!(data, b"x");
        assert!(replies.is_empty());
    }

    /// En IAC-sekvens fragmenterad över flera `process`-anrop (motsvarar
    /// flera TCP-läsningar) måste hanteras korrekt — staten får inte
    /// glömmas mellan anropen.
    #[test]
    fn negotiation_fragmented_across_multiple_reads() {
        let mut filter = IacFilter::new();
        let (data1, replies1) = filter.process(&[255]);
        assert!(data1.is_empty());
        assert!(replies1.is_empty());
        let (data2, replies2) = filter.process(&[251]);
        assert!(data2.is_empty());
        assert!(replies2.is_empty());
        let (data3, replies3) = filter.process(&[1]);
        assert!(data3.is_empty());
        assert_eq!(replies3, vec![255, 254, 1]);
    }

    /// GENUIN end-to-end mot en riktig TCP-server som beter sig som en
    /// telnet-server: skickar en förhandling (WILL ECHO) klienten ska
    /// refusera, sen text klienten ska ta emot rensad — och läser klientens
    /// svar direkt av tråden. Bevisar att hela vägen (socket, IAC-filter,
    /// async_channel) fungerar ihop, inte bara filtrets logik i isolering.
    #[tokio::test]
    async fn connect_strips_negotiation_and_delivers_clean_data() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("kunde inte binda testservern");
        let port = listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept misslyckades");
            let mut hello = vec![255, 251, 1]; // IAC WILL ECHO
            hello.extend_from_slice(b"welcome\r\n");
            socket.write_all(&hello).await.expect("kunde inte skriva hälsning");

            let mut buf = [0u8; 64];
            let n = socket.read(&mut buf).await.expect("kunde inte läsa klientens svar");
            buf[..n].to_vec()
        });

        let handle = spawn("127.0.0.1".to_string(), port);
        let first = tokio::time::timeout(std::time::Duration::from_secs(5), handle.output.recv())
            .await
            .expect("timeout väntade på Connected")
            .expect("kanalen stängdes oväntat");
        assert!(matches!(first, TelnetEvent::Connected));

        let second = tokio::time::timeout(std::time::Duration::from_secs(5), handle.output.recv())
            .await
            .expect("timeout väntade på data")
            .expect("kanalen stängdes oväntat");
        match second {
            TelnetEvent::Data(bytes) => {
                assert_eq!(bytes, b"welcome\r\n", "servens förhandling ska vara borttagen, bara texten kvar")
            }
            other => panic!("väntade Data, fick {other:?}"),
        }

        let reply = tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .expect("timeout väntade på serverns mottagning")
            .expect("servertråden panikade");
        assert_eq!(reply, vec![255, 254, 1]); // IAC DONT ECHO
    }
}
