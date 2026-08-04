//! Wake-on-LAN — bygger och skickar ett "magic packet" för att väcka en
//! avstängd/vilande maskin på det lokala nätverket. Ren UDP, ingen SSH-
//! koppling alls, port av `Sources/SSHCore/WakeOnLan.swift` (samma
//! byte-för-byte-format, samma felkontrakt).
//!
//! `Host.mac_address` (`host.rs`) fanns redan i datamodellen (för
//! wire-format-kompatibilitet med App/Windows-synk) men lästes tidigare
//! aldrig av något i LinuxApp.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakeOnLanError {
    InvalidMacAddress(String),
    /// `port` utanför `1..=65535` — plattformens sockets-lager tolkar annars
    /// ofta talet modulo 65536 istället för att felar (t.ex. 70000 blir 4464
    /// på Linux), vilket tyst skulle skicka paketet till FEL port istället
    /// för att misslyckas synligt — samma motivering som Swift-sidans
    /// `WakeOnLanError.invalidPort`.
    InvalidPort(u32),
    Io(String),
}

impl std::fmt::Display for WakeOnLanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WakeOnLanError::InvalidMacAddress(mac) => write!(f, "ogiltig MAC-adress: {mac}"),
            WakeOnLanError::InvalidPort(port) => write!(f, "ogiltig port: {port}"),
            WakeOnLanError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Parsar en MAC-adress i valfritt vanligt format (`:`, `-` eller inget
/// separatortecken, skiftlägesokänsligt) till 6 råa bytes.
pub fn parse_mac(mac: &str) -> Result<[u8; 6], WakeOnLanError> {
    let cleaned: String = mac.chars().filter(|&c| c != ':' && c != '-').collect();
    if cleaned.len() != 12 || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(WakeOnLanError::InvalidMacAddress(mac.to_string()));
    }
    let mut bytes = [0u8; 6];
    for i in 0..6 {
        bytes[i] = u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16)
            .map_err(|_| WakeOnLanError::InvalidMacAddress(mac.to_string()))?;
    }
    Ok(bytes)
}

/// Det klassiska magic packet-formatet: 6 bytes `0xFF` följt av MAC-adressen
/// upprepad 16 gånger (102 bytes totalt).
pub fn magic_packet(mac: &str) -> Result<Vec<u8>, WakeOnLanError> {
    let mac_bytes = parse_mac(mac)?;
    let mut packet = vec![0xFFu8; 6];
    for _ in 0..16 {
        packet.extend_from_slice(&mac_bytes);
    }
    Ok(packet)
}

/// Skickar magic packet via UDP-broadcast. `broadcast_address` är
/// nätverkets broadcast-adress (t.ex. `192.168.1.255`), inte målets egen
/// IP — enheten svarar inte på ARP i sovande/avstängt läge, så adressering
/// måste ske via broadcast. Standardport 9 (`discard`) matchar de flesta
/// WoL-implementationers förväntan.
pub async fn send(
    mac: &str,
    broadcast_address: &str,
    port: u32,
) -> Result<(), WakeOnLanError> {
    if !(1..=65_535).contains(&port) {
        return Err(WakeOnLanError::InvalidPort(port));
    }
    let packet = magic_packet(mac)?;
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| WakeOnLanError::Io(e.to_string()))?;
    socket
        .set_broadcast(true)
        .map_err(|e| WakeOnLanError::Io(e.to_string()))?;
    socket
        .send_to(&packet, (broadcast_address, port as u16))
        .await
        .map_err(|e| WakeOnLanError::Io(e.to_string()))?;
    Ok(())
}

/// Startar `send` på en egen bakgrundstråd med sin egen tokio-runtime —
/// samma mönster som `ssh::spawn_shell`/`ssh::run_command`: GTK:s huvudloop
/// (`glib::spawn_future_local`) har INGEN egen tokio-reaktor, så `send`s
/// `tokio::net::UdpSocket` kan inte köras direkt där (skulle panika:
/// "there is no reactor running"). Kommunicerar tillbaka via
/// `async_channel`, som är exekverar-agnostisk och funkar fint att `.await`a
/// från `glib::spawn_future_local`.
pub fn spawn_send(
    mac: String,
    broadcast_address: String,
    port: u32,
) -> async_channel::Receiver<Result<(), WakeOnLanError>> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för wake-on-lan-tråden");
        let result = rt.block_on(send(&mac, &broadcast_address, port));
        let _ = tx.send_blocking(result);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mac_with_colons() {
        assert_eq!(
            parse_mac("AA:BB:CC:DD:EE:FF").unwrap(),
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]
        );
    }

    #[test]
    fn parse_mac_with_dashes() {
        assert_eq!(
            parse_mac("aa-bb-cc-dd-ee-ff").unwrap(),
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]
        );
    }

    #[test]
    fn parse_mac_without_separators() {
        assert_eq!(
            parse_mac("aabbccddeeff").unwrap(),
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]
        );
    }

    #[test]
    fn parse_mac_rejects_wrong_length() {
        assert_eq!(
            parse_mac("AA:BB:CC:DD:EE"),
            Err(WakeOnLanError::InvalidMacAddress("AA:BB:CC:DD:EE".to_string()))
        );
    }

    #[test]
    fn parse_mac_rejects_non_hex_characters() {
        assert!(parse_mac("ZZ:BB:CC:DD:EE:FF").is_err());
    }

    #[test]
    fn magic_packet_format() {
        let packet = magic_packet("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(packet.len(), 102);
        assert_eq!(&packet[0..6], &[0xFF; 6]);
        let mac = [0xAAu8, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        for i in 0..16 {
            let start = 6 + i * 6;
            assert_eq!(&packet[start..start + 6], &mac, "repetition {i}");
        }
    }

    /// Plattformens sockets-lager tolkar annars ofta ett portnummer utanför
    /// giltigt intervall modulo 65536 (t.ex. 70000 -> 4464) istället för att
    /// felar — paketet skulle tyst gå till FEL port. Måste valideras explicit.
    #[tokio::test]
    async fn send_rejects_out_of_range_port() {
        let err = send("AA:BB:CC:DD:EE:FF", "127.0.0.1", 70_000)
            .await
            .expect_err("70000 ska avvisas som ogiltig port");
        assert_eq!(err, WakeOnLanError::InvalidPort(70_000));
    }

    #[tokio::test]
    async fn send_rejects_zero_port() {
        let err = send("AA:BB:CC:DD:EE:FF", "127.0.0.1", 0)
            .await
            .expect_err("0 ska avvisas som ogiltig port");
        assert_eq!(err, WakeOnLanError::InvalidPort(0));
    }

    /// End-to-end mot en RIKTIG UDP-lyssnare på loopback — bevisar att
    /// paketet faktiskt går ut på tråden med rätt innehåll, inte bara att
    /// byte-layouten stämmer i minnet. `set_broadcast(true)` fungerar även
    /// mot en unicast-adress som 127.0.0.1 (broadcast-flaggan begränsar
    /// bara vilka adresser som TILLÅTS, kräver den inte).
    #[tokio::test]
    async fn send_delivers_magic_packet_over_real_udp() {
        let listener = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("kunde inte binda testlyssnaren");
        let port = listener.local_addr().unwrap().port();

        send("AA:BB:CC:DD:EE:FF", "127.0.0.1", port as u32)
            .await
            .expect("send misslyckades");

        let mut buf = [0u8; 256];
        let (n, _) = tokio::time::timeout(std::time::Duration::from_secs(5), listener.recv_from(&mut buf))
            .await
            .expect("timeout väntade på magic packet")
            .expect("recv_from misslyckades");
        assert_eq!(&buf[..n], magic_packet("AA:BB:CC:DD:EE:FF").unwrap().as_slice());
    }
}
