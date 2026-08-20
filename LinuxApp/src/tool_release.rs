//! Var en extern binär hämtas ifrån, per verktyg och plattform.
//!
//! Kompletterar [`crate::external_binary_fetcher`], som är medvetet generisk
//! (URL + förväntad checksumma in, verifierad sökväg ut). Den här modulen bär
//! den verktygsspecifika kunskapen: vilken URL, vilket filnamn för den här
//! arkitekturen, och var checksumman står.
//!
//! Motiveras av VISION.md "Native WireGuard/Tailscale — inget externt
//! beroende": Bastion ska kunna hämta verktygen själv i stället för att kräva
//! att användaren installerat dem separat, och versionsväljaren i samma
//! VISION-punkt kräver att man kan fråga vilka versioner som finns.
//!
//! # Vad som faktiskt går att hämta, mätt 2026-08-19
//!
//! **Tailscale: ja.** `https://pkgs.tailscale.com/{kanal}/?mode=json` ger ett
//! index med `TarballsVersion` och en `Tarballs`-karta från arkitektur till
//! filnamn, och bredvid varje tarball ligger `<filnamn>.sha256` med ett rått
//! hexvärde. Verifierat mot den riktiga tjänsten, inte läst i dokumentation.
//!
//! **wireguard-go: nej.** ROADMAP.md påstod att den "finns för i princip alla
//! plattformar som EN binär". Det stämmer inte: projektet publicerar bara
//! KÄLLKODSTARBALLS (`wireguard-go-<datum>.tar.xz` från git.zx2c4.com) och
//! har inga binärsläpp alls. En färdig binär kräver alltså antingen en
//! Go-verktygskedja på användarens maskin eller ett tredjepartsbygge vi skulle
//! få lita på — två helt andra tillitsfrågor än "ladda ner och verifiera mot
//! projektets egen checksumma". Därför täcker den här modulen Tailscale nu
//! och lämnar WireGuard öppen; se ROADMAP.md.
//!
//! `#![allow(dead_code)]` av exakt samma skäl som
//! [`crate::external_binary_fetcher`]: modulen är den andra byggstenen i
//! samma kedja och väntar på ett UI-lager som anropar den. Att koppla in en
//! nedladdningsknapp innan upplösningen är bevisat korrekt vore att bygga
//! ordningen baklänges.
#![allow(dead_code)]

/// Tailscales två publika utgivningskanaler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Unstable,
}

impl Channel {
    fn path(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Unstable => "unstable",
        }
    }
}

const TAILSCALE_BASE: &str = "https://pkgs.tailscale.com";

/// Indexet som listar vad kanalen just nu erbjuder.
pub fn tailscale_index_url(channel: Channel) -> String {
    format!("{TAILSCALE_BASE}/{}/?mode=json", channel.path())
}

/// En hämtbar Tailscale-utgåva för en bestämd arkitektur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailscaleRelease {
    pub version: String,
    pub file_name: String,
    pub download_url: String,
    /// Checksumman ligger i en egen fil bredvid tarballen. Den hämtas över
    /// HTTPS från samma värd som binären, vilket är den praktiska
    /// tillitsgränsen — den skyddar mot ett trasigt eller manipulerat
    /// MELLANLED, inte mot att Tailscale själva skulle publicera fel binär.
    /// Att låtsas annat vore att övertolka vad en checksumma bevisar.
    pub checksum_url: String,
}

/// Läser indexet och plockar ut utgåvan för `arch`.
///
/// `arch` är Tailscales egen namngivning (`amd64`, `arm64`, `386` …), inte
/// Rusts — använd [`host_arch`] för att översätta.
pub fn tailscale_release(
    index_json: &str,
    channel: Channel,
    arch: &str,
) -> Result<TailscaleRelease, String> {
    let index: serde_json::Value =
        serde_json::from_str(index_json).map_err(|e| format!("kunde inte läsa indexet: {e}"))?;

    let version = index
        .get("TarballsVersion")
        .and_then(|v| v.as_str())
        .ok_or("indexet saknar TarballsVersion")?
        .to_string();

    let file_name = index
        .get("Tarballs")
        .and_then(|t| t.get(arch))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            // Listar vad som FINNS. Ett "stöds inte" utan alternativ säger
            // inget om huruvida det är arkitekturen eller kanalen som är fel.
            let available = index
                .get("Tarballs")
                .and_then(|t| t.as_object())
                .map(|o| o.keys().cloned().collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            format!("ingen tarball för arkitekturen {arch:?}; kanalen erbjuder: {available}")
        })?
        .to_string();

    let base = format!("{TAILSCALE_BASE}/{}/{file_name}", channel.path());
    Ok(TailscaleRelease {
        version,
        file_name,
        checksum_url: format!("{base}.sha256"),
        download_url: base,
    })
}

/// Översätter Rusts arkitekturnamn till Tailscales.
///
/// `None` för arkitekturer Tailscale inte bygger för — bättre än att gissa
/// ett namn som ger en 404 långt senare i flödet.
pub fn host_arch() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("amd64"),
        "aarch64" => Some("arm64"),
        "x86" => Some("386"),
        "arm" => Some("arm"),
        "riscv64" => Some("riscv64"),
        "mips64" => Some("mips64"),
        _ => None,
    }
}

/// Läser en `.sha256`-fil.
///
/// Tailscale skriver ett RÅTT hexvärde utan filnamn efter, till skillnad från
/// `sha256sum`-formatet. Båda accepteras här — den som lägger till en ny källa
/// ska inte behöva upptäcka skillnaden genom en checksummeavvikelse.
pub fn parse_checksum(body: &str) -> Result<String, String> {
    let first = body
        .split_whitespace()
        .next()
        .ok_or("checksummefilen var tom")?
        .to_ascii_lowercase();
    if first.len() != 64 || !first.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "checksumman ser inte ut som SHA256 (64 hextecken): {first:?}"
        ));
    }
    Ok(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Riktigt index, hämtat från den skarpa tjänsten 2026-08-19. Inte
    /// handskrivet: formen är det enda vi inte kontrollerar själva, så en
    /// påhittad fixtur hade bevisat att koden läser sitt eget antagande.
    fn real_index() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../Tests/fixtures/tailscale-stable-index.json");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("kunde inte läsa {}: {e}", path.display()))
    }

    #[test]
    fn a_real_index_yields_a_complete_download_and_checksum_url() {
        let r = tailscale_release(&real_index(), Channel::Stable, "amd64").unwrap();
        assert!(!r.version.is_empty());
        assert!(r.file_name.starts_with("tailscale_"));
        assert!(r.file_name.ends_with("_amd64.tgz"), "fick {}", r.file_name);
        assert_eq!(
            r.download_url,
            format!("https://pkgs.tailscale.com/stable/{}", r.file_name)
        );
        assert_eq!(r.checksum_url, format!("{}.sha256", r.download_url));
    }

    /// Versionen i filnamnet och den indexet uppger måste vara samma. Går de
    /// isär har vi byggt en URL till en annan utgåva än den vi tror.
    #[test]
    fn the_file_name_carries_the_same_version_the_index_reports() {
        let r = tailscale_release(&real_index(), Channel::Stable, "arm64").unwrap();
        assert!(
            r.file_name.contains(&r.version),
            "filnamnet {:?} nämner inte versionen {:?}",
            r.file_name,
            r.version
        );
    }

    /// Kanalen ska synas i URL:en, annars laddas alltid stable ner oavsett
    /// vad användaren valt i versionsväljaren.
    #[test]
    fn the_channel_reaches_the_url() {
        assert!(tailscale_index_url(Channel::Unstable).contains("/unstable/"));
        let r = tailscale_release(&real_index(), Channel::Unstable, "amd64").unwrap();
        assert!(r.download_url.contains("/unstable/"), "fick {}", r.download_url);
    }

    /// Ett fel ska säga vad som FINNS. "Stöds inte" utan alternativ går inte
    /// att agera på — det syns inte om det är arkitekturen eller kanalen som
    /// är fel.
    #[test]
    fn an_unknown_architecture_names_the_ones_that_exist() {
        let err = tailscale_release(&real_index(), Channel::Stable, "sparc").unwrap_err();
        assert!(err.contains("sparc"), "fick {err}");
        assert!(err.contains("amd64"), "felet ska lista vad som finns: {err}");
    }

    #[test]
    fn a_malformed_index_is_an_error_not_a_panic() {
        assert!(tailscale_release("{", Channel::Stable, "amd64").is_err());
        assert!(tailscale_release("{}", Channel::Stable, "amd64").is_err());
        assert!(tailscale_release(r#"{"TarballsVersion":"1.0"}"#, Channel::Stable, "amd64").is_err());
    }

    /// Den här maskinens arkitektur ska finnas i det riktiga indexet —
    /// annars är översättningstabellen fel för just den plattform vi kör på.
    #[test]
    fn this_machines_architecture_resolves_against_the_real_index() {
        let Some(arch) = host_arch() else {
            eprintln!("hoppar: {} är inte en arkitektur Tailscale bygger för", std::env::consts::ARCH);
            return;
        };
        assert!(
            tailscale_release(&real_index(), Channel::Stable, arch).is_ok(),
            "host_arch() gav {arch:?}, som indexet inte känner igen"
        );
    }

    /// Tailscale skriver ett rått hexvärde; `sha256sum` skriver hex + filnamn.
    /// Båda ska läsas, och allt annat ska avvisas i stället för att skickas
    /// vidare som en "checksumma" ingenting kan matcha.
    #[test]
    fn both_checksum_formats_are_read_and_junk_is_rejected() {
        let hex = "ad2cde12f8de95f7b93a1e0401e652291c603d42b9d60a33fb1741eb38ab04d8";
        assert_eq!(parse_checksum(hex).unwrap(), hex);
        assert_eq!(parse_checksum(&format!("{hex}\n")).unwrap(), hex);
        assert_eq!(parse_checksum(&format!("{hex}  tailscale.tgz\n")).unwrap(), hex);
        assert_eq!(parse_checksum(&hex.to_uppercase()).unwrap(), hex, "hex är skiftlägesokänsligt");

        assert!(parse_checksum("").is_err());
        assert!(parse_checksum("inte-hex").is_err());
        assert!(parse_checksum(&hex[..63]).is_err(), "för kort är fel längd, inte 'nästan rätt'");
        assert!(parse_checksum(&format!("{hex}0")).is_err(), "för lång likaså");
    }

    /// Mot den SKARPA tjänsten: fixturen ovan låser formen, det här beviset
    /// att formen fortfarande gäller. Ignorerad som standard eftersom den
    /// kräver nät och därmed inte hör hemma i en vanlig testkörning.
    #[tokio::test]
    #[ignore = "kräver nätverk mot pkgs.tailscale.com"]
    async fn the_live_service_still_serves_the_shape_we_expect() {
        let client = reqwest::Client::new();
        let index = client
            .get(tailscale_index_url(Channel::Stable))
            .send()
            .await
            .expect("kunde inte nå pkgs.tailscale.com")
            .text()
            .await
            .unwrap();
        let release = tailscale_release(&index, Channel::Stable, "amd64")
            .expect("den skarpa tjänsten svarade i en form vi inte längre förstår");

        let sum = client
            .get(&release.checksum_url)
            .send()
            .await
            .expect("checksummefilen gick inte att hämta")
            .text()
            .await
            .unwrap();
        parse_checksum(&sum).expect("checksummefilen hade inte det format vi läser");
    }
}
