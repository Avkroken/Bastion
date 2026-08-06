//! Testhjälpmedel som delas av modulernas egna `mod tests`.
//!
//! Finns av EN anledning: sex olika tester startar var sin fristående
//! test-sshd, och både portvalet och väntan på att den ska lyssna måste
//! samordnas mellan dem. `cargo test` kör hela sviten i EN process med
//! flera trådar, så "välj en ledig port" är inte en lokal fråga — två
//! tester som väljer var för sig kan välja SAMMA port.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

fn reserved_ports() -> &'static Mutex<HashSet<u16>> {
    static PORTS: OnceLock<Mutex<HashSet<u16>>> = OnceLock::new();
    PORTS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Väljer en ledig TCP-port och RESERVERAR den för anroparen inom den
/// här processen.
///
/// sshd kan inte binda port 0 (OS-tilldelad), så en riktig ledig port
/// måste hittas i förväg genom att binda och släppa. Det lämnar ett
/// glapp där porten är ledig igen — och utan reservationen kunde två
/// samtidiga tester få samma port ur det glappet. Då vinner den ena
/// sshd:n bindningen, den andra dör direkt, och den förlorande sidan
/// såg tidigare en LYCKAD anslutning till grannens sshd innan den
/// föll med "Connection refused" så fort grannen städades bort.
///
/// De redan avvisade lyssnarna hålls kvar tills en oreserverad port
/// hittats, så varje varv garanterat får en ny port i stället för att
/// kunna få tillbaka samma ledig port om och om igen.
///
/// Reservationen släpps aldrig. Testbinären är kortlivad och antalet
/// sshd-tester är litet, och att återanvända en port efter att dess
/// sshd dött skulle bara återinföra racet.
pub fn reserve_port() -> Option<u16> {
    let mut rejected = Vec::new();
    let mut chosen = None;

    for _ in 0..100 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
        let port = listener.local_addr().ok()?.port();
        if reserved_ports().lock().ok()?.insert(port) {
            chosen = Some(port);
            break;
        }
        rejected.push(listener);
    }

    // Släpps först här: så länge de lever kan OS:et inte dela ut samma
    // port igen, vilket är det som driver sökningen framåt.
    drop(rejected);
    chosen
}

/// known_hosts-filen som HELA testbinären använder, i stället för den
/// riktiga `~/.bastion/known_hosts`.
///
/// Utan den här skrev testsviten i användarens skarpa fil: varje
/// test-sshd får en nygenererad värdnyckel på en slumpad port, och TOFU
/// lärde in `127.0.0.1:<port>` permanent. Effekten var två fel, båda
/// verkliga:
///
/// 1. Filen växte med ett antal rader per körning — 411 rader rent
///    testskräp innan det upptäcktes.
/// 2. När portintervallet varvade runt och en port återanvändes med en
///    NY värdnyckel avslog TOFU anslutningen med "VÄRDNYCKELN HAR
///    ÄNDRATS" — ett test som föll av ett äkta säkerhetslarm mot
///    kvarlämnad state från en TIDIGARE körning.
///
/// Sökvägen är ny för varje processkörning, så sviten alltid startar
/// utan inlärda värdar. Tillsammans med [`reserve_port`] — som håller
/// portarna unika INOM en körning — kan två test-sshd:er varken krocka
/// med varandra eller med historiken.
pub fn known_hosts_path() -> std::path::PathBuf {
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        std::env::temp_dir().join(format!("bastion-test-known-hosts-{}", uuid::Uuid::new_v4()))
    })
    .clone()
}

/// Väntar tills VÅR sshd faktiskt lyssnar på `port`.
///
/// Skiljer sig från en naiv `TcpStream::connect`-loop på den punkt som
/// är hela poängen: den kontrollerar först att barnprocessen fortfarande
/// LEVER. En sshd som inte fick binda porten avslutar omedelbart, och
/// utan livskontrollen kan loopen då ansluta till någon ANNANS lyssnare
/// på samma port och rapportera framgång.
///
/// Vid misslyckande dödas och skördas processen här, så en sshd som
/// aldrig kom igång inte blir kvar som en föräldralös process — den
/// tidigare loopen returnerade bara `None` och lämnade den vid liv,
/// eftersom `Child` inte dödar vid drop.
pub fn wait_until_listening(child: &mut std::process::Child, port: u16) -> bool {
    for _ in 0..50 {
        match child.try_wait() {
            // sshd har avslutat — typiskt "Address already in use".
            Ok(Some(_)) | Err(_) => break,
            Ok(None) => {}
        }
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let _ = child.kill();
    let _ = child.wait();
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Två anrop får ALDRIG samma port — det är exakt den kollisionen
    /// som gjorde test-sshd:erna flakiga.
    #[test]
    fn reserve_port_never_hands_out_the_same_port_twice() {
        let a = reserve_port().expect("en ledig port ska gå att reservera");
        let b = reserve_port().expect("en ledig port ska gå att reservera");
        assert_ne!(a, b, "reserve_port delade ut samma port två gånger");
    }

    /// Samtidiga anrop från flera trådar ska ge unika portar — enkla
    /// varianten (bind-och-släpp utan delad bokföring) klarar inte det.
    #[test]
    fn concurrent_callers_get_unique_ports() {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| reserve_port().expect("en ledig port ska gå att reservera"))
            })
            .collect();
        let ports: Vec<u16> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        let unique: HashSet<u16> = ports.iter().copied().collect();
        assert_eq!(
            unique.len(),
            ports.len(),
            "portar krockade mellan trådar: {ports:?}"
        );
    }

    /// En process som redan dött ska rapporteras som "lyssnar inte",
    /// även om något annat råkar lyssna på porten.
    #[test]
    fn a_dead_child_is_never_reported_as_listening() {
        let port = reserve_port().expect("en ledig port ska gå att reservera");
        // Någon ANNAN lyssnar på porten — grannens sshd, i praktiken.
        let _squatter = std::net::TcpListener::bind(("127.0.0.1", port)).expect("ska gå att binda");

        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("`true` ska gå att starta");
        let _ = child.wait();

        assert!(
            !wait_until_listening(&mut child, port),
            "en död sshd rapporterades som igång bara för att porten svarade"
        );
    }
}
