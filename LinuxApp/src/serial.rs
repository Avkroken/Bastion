//! Seriell/USB-anslutning (t.ex. `/dev/ttyUSB*`/`/dev/ttyACM*` på Linux) —
//! Termius har detta, Bastion saknade det helt. Port av
//! `Sources/SSHCore/Serial.swift`. Mest relevant för hemmalabb-/
//! nätverksutrustnings-konsolportar (samma "nätverkstekniker"-persona som
//! Telnet, se VISION.md).
//!
//! Till skillnad från `ssh.rs`/`telnet.rs` (en enda bakgrundstråd,
//! `tokio::select!` mellan inkommande/utgående) används HÄR två separata
//! trådar utan tokio alls: seriella filbeskrivare är vanliga blockerande
//! syscall-baserade I/O-primitiver (`std::fs::File::read`/`write` fungerar
//! direkt när porten väl är konfigurerad via termios), och en dedikerad
//! blockerande läs-tråd är enklare OCH effektivare än Swift-sidans
//! `NIOPipeBootstrap`-väg (som behöver icke-blockerande I/O för att passa
//! NIOs event-loop-modell — det behovet finns inte här).

use std::io::{Read, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SerialConfig {
    pub path: String,
    pub baud_rate: u32,
}

/// De vanligaste standardbaudhastigheterna — gemensamma för Glibcs
/// `termios.h`-konstanter (ovanligare/plattformsspecifika hastigheter som
/// 460800/921600 stöds medvetet inte, samma avgränsning som Swift-sidan).
pub const COMMON_BAUD_RATES: [u32; 10] =
    [300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerialError {
    OpenFailed(String),
    ConfigurationFailed(String),
    UnsupportedBaudRate(u32),
}

impl std::fmt::Display for SerialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SerialError::OpenFailed(path) => write!(f, "kunde inte öppna {path}"),
            SerialError::ConfigurationFailed(msg) => write!(f, "{msg}"),
            SerialError::UnsupportedBaudRate(rate) => write!(f, "{rate} baud stöds inte"),
        }
    }
}

#[derive(Debug)]
pub enum SerialEvent {
    Connected,
    Data(Vec<u8>),
    Error(String),
    Closed,
}

pub struct SerialHandle {
    pub input: async_channel::Sender<Vec<u8>>,
    pub output: async_channel::Receiver<SerialEvent>,
}

fn speed_for_baud_rate(baud_rate: u32) -> Result<libc::speed_t, SerialError> {
    Ok(match baud_rate {
        300 => libc::B300,
        1200 => libc::B1200,
        2400 => libc::B2400,
        4800 => libc::B4800,
        9600 => libc::B9600,
        19200 => libc::B19200,
        38400 => libc::B38400,
        57600 => libc::B57600,
        115200 => libc::B115200,
        230400 => libc::B230400,
        other => return Err(SerialError::UnsupportedBaudRate(other)),
    })
}

/// Sätter porten i "rått" läge (`cfmakeraw`: ingen kanonisk radbuffring,
/// inget lokalt eko, inga signalkontrolltecken tolkade) — exakt det en
/// generisk terminalvy mot godtycklig seriell utrustning behöver, samma
/// resonemang som `telnet::IacFilter`s "avvisa alla NVT-alternativ".
/// `CLOCAL`/`CREAD` sätts explicit: ignorera modemstatuslinjer och
/// aktivera mottagning — utan dem kan `open()` blockera eller kanalen
/// aldrig ta emot data på vissa seriella drivrutiner.
fn configure_termios(fd: RawFd, baud_rate: u32) -> Result<(), SerialError> {
    unsafe {
        let mut raw: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut raw) != 0 {
            return Err(SerialError::ConfigurationFailed("tcgetattr misslyckades".to_string()));
        }
        libc::cfmakeraw(&mut raw);
        let speed = speed_for_baud_rate(baud_rate)?;
        libc::cfsetispeed(&mut raw, speed);
        libc::cfsetospeed(&mut raw, speed);
        raw.c_cflag |= libc::CLOCAL | libc::CREAD;
        if libc::tcsetattr(fd, libc::TCSANOW, &raw) != 0 {
            return Err(SerialError::ConfigurationFailed("tcsetattr misslyckades".to_string()));
        }
    }
    Ok(())
}

/// Öppnar och konfigurerar porten, returnerar den råa filbeskrivaren.
/// `O_NONBLOCK` sätts vid `open()` (krävs av vissa seriella drivrutiner för
/// att `open()` inte ska blockera i väntan på en modemstatuslinje som
/// aldrig kommer — samma skäl som Swift-sidan) men rensas bort direkt
/// efteråt via `fcntl` — läsningen sker sedan på en dedikerad blockerande
/// tråd (se modulkommentaren), som INTE ska busy-loopa på `EWOULDBLOCK`.
fn open_and_configure(config: &SerialConfig) -> Result<RawFd, SerialError> {
    let c_path = std::ffi::CString::new(config.path.clone())
        .map_err(|_| SerialError::OpenFailed(config.path.clone()))?;
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK) };
    if fd < 0 {
        return Err(SerialError::OpenFailed(config.path.clone()));
    }
    if let Err(e) = configure_termios(fd, config.baud_rate) {
        unsafe { libc::close(fd) };
        return Err(e);
    }
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
        }
    }
    Ok(fd)
}

/// Startar en seriell session på egna bakgrundstrådar (en läs-, en
/// skriv-tråd — se modulkommentaren för varför ingen tokio-runtime behövs
/// här, till skillnad från `ssh.rs`/`telnet.rs`).
pub fn spawn(config: SerialConfig) -> SerialHandle {
    let (input_tx, input_rx) = async_channel::unbounded::<Vec<u8>>();
    let (output_tx, output_rx) = async_channel::unbounded::<SerialEvent>();
    let stop = Arc::new(AtomicBool::new(false));

    std::thread::spawn(move || {
        let fd = match open_and_configure(&config) {
            Ok(fd) => fd,
            Err(e) => {
                let _ = output_tx.send_blocking(SerialEvent::Error(e.to_string()));
                let _ = output_tx.send_blocking(SerialEvent::Closed);
                return;
            }
        };

        // `dup`: skriv- och läs-tråden behöver var sin oberoende `File`
        // (båda pekar på samma underliggande öppna filbeskrivning i
        // kärnan, så skrivningar/läsningar går till samma port) — annars
        // skulle två `File`-värden dela ÄGARSKAP av samma fd-heltal, och
        // den ena som droppas (stänger fd:t) hade brutit den andra.
        let write_fd = unsafe { libc::dup(fd) };

        {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut file = unsafe { std::fs::File::from_raw_fd(write_fd) };
                while let Ok(bytes) = input_rx.recv_blocking() {
                    if file.write_all(&bytes).is_err() {
                        break;
                    }
                }
                // Kanalen stängdes (fliken stängdes) — signalera läs-tråden
                // att sluta nästa gång den vaknar (se pollningsloopen
                // nedan). `file` går ur scope här och stänger `write_fd`.
                stop.store(true, Ordering::SeqCst);
            });
        }

        let _ = output_tx.send_blocking(SerialEvent::Connected);
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        let mut buf = [0u8; 4096];
        loop {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            match file.read(&mut buf) {
                Ok(0) => break, // EOF — enheten kopplades bort
                Ok(n) => {
                    if output_tx.send_blocking(SerialEvent::Data(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = output_tx.send_blocking(SerialEvent::Error(e.to_string()));
                    break;
                }
            }
        }
        let _ = output_tx.send_blocking(SerialEvent::Closed);
    });

    SerialHandle { input: input_tx, output: output_rx }
}

/// Listar sannolika seriella enheter för en "välj port"-UI — best-effort,
/// ingen garanti att en listad enhet faktiskt är en riktig seriell adapter.
/// Enkel katalogläsning, inget beroende på `udev`.
pub fn available_paths() -> Vec<String> {
    let prefixes = ["ttyUSB", "ttyACM", "ttyS"];
    let Ok(entries) = std::fs::read_dir("/dev") else { return Vec::new() };
    let mut paths: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| prefixes.iter().any(|p| name.starts_with(p)))
        .map(|name| format!("/dev/{name}"))
        .collect();
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_nonexistent_path_throws() {
        let config = SerialConfig { path: "/dev/does-not-exist-bastion-test".to_string(), baud_rate: 9600 };
        let result = open_and_configure(&config);
        assert_eq!(
            result,
            Err(SerialError::OpenFailed("/dev/does-not-exist-bastion-test".to_string()))
        );
    }

    #[test]
    fn unsupported_baud_rate_is_rejected() {
        assert_eq!(speed_for_baud_rate(42), Err(SerialError::UnsupportedBaudRate(42)));
    }

    #[test]
    fn common_baud_rates_are_all_accepted() {
        for &rate in &COMMON_BAUD_RATES {
            assert!(speed_for_baud_rate(rate).is_ok(), "{rate} baud borde accepteras");
        }
    }

    /// Öppnar en RIKTIG pseudo-terminal (PTY) via `posix_openpt`/`grantpt`/
    /// `unlockpt`/`ptsname` — inte en mock. En PTY-slav beter sig som en
    /// äkta seriell tty ur `open()`/`tcsetattr()`s perspektiv (samma
    /// drivrutinsfamilj i kärnan), så det här bevisar att
    /// `configure_termios`/`open_and_configure`-vägen faktiskt fungerar
    /// mot en riktig enhet, inte bara att koden kompilerar — samma teknik
    /// som Swift-sidans `SerialPTYTests` (där begränsad till Darwin av ett
    /// lokalt toolchain-skäl, inte för att Linux saknar syscallen).
    fn open_pty_pair() -> (RawFd, String) {
        unsafe {
            let master_fd = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
            assert!(master_fd >= 0, "posix_openpt misslyckades");
            assert_eq!(libc::grantpt(master_fd), 0, "grantpt misslyckades");
            assert_eq!(libc::unlockpt(master_fd), 0, "unlockpt misslyckades");
            let name_ptr = libc::ptsname(master_fd);
            assert!(!name_ptr.is_null(), "ptsname misslyckades");
            let slave_path = std::ffi::CStr::from_ptr(name_ptr).to_string_lossy().into_owned();
            (master_fd, slave_path)
        }
    }

    #[test]
    fn send_receive_round_trip_over_a_real_pty() {
        let (master_fd, slave_path) = open_pty_pair();
        let mut master = unsafe { std::fs::File::from_raw_fd(master_fd) };

        let handle = spawn(SerialConfig { path: slave_path, baud_rate: 9600 });

        // Vänta in `Connected` INNAN något skrivs till master — annars kan
        // skrivningen hinna före `configure_termios` (rått läge) någonsin
        // körts, vilket lämnar porten i kärnans DEFAULT cooked-läge
        // (echo/icrnl/onlcr m.m. påslagna) och ger korrupt/omvandlad data.
        // Bevisat empiriskt: utan denna väntan flaggar testet ibland en
        // extra `\r` inskjuten före `\n` — precis den sortens
        // cooked-läge-artefakt `cfmakeraw` finns till för att förhindra.
        match handle.output.recv_blocking() {
            Ok(SerialEvent::Connected) => {}
            other => panic!("väntade Connected, fick {other:?}"),
        }

        // Master -> handle.output ("data kommer in från den seriella enheten").
        master.write_all(b"hej\n").expect("kunde inte skriva till master");
        let mut received = Vec::new();
        loop {
            match handle.output.recv_blocking() {
                Ok(SerialEvent::Data(bytes)) => {
                    received.extend_from_slice(&bytes);
                    if received.len() >= 4 {
                        break;
                    }
                }
                other => panic!("väntade Data, fick {other:?}"),
            }
        }
        assert_eq!(received, b"hej\n");

        // handle.input -> master ("data skickas UT till den seriella enheten").
        handle.input.send_blocking(b"echo\n".to_vec()).expect("kunde inte skicka");
        let mut read_buf = [0u8; 64];
        let mut total = 0;
        for _ in 0..50 {
            match master.read(&mut read_buf) {
                Ok(n) if n > 0 => {
                    total = n;
                    break;
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        assert_eq!(&read_buf[..total], b"echo\n");
    }
}
