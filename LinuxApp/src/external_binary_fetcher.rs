//! Hämtar och cachar en enskild extern binär (t.ex. `wireguard-go`,
//! `tailscale`) från en URL, verifierad mot en känd SHA256-checksumma
//! INNAN den någonsin skrivs till disk som "giltig". Port av
//! `Sources/SSHCore/ExternalBinaryFetcher.swift`.
//!
//! **Används INTE än någonstans** — varken här eller i Swift-referensen.
//! Motiveras av VISION.md "Native WireGuard/Tailscale — inget externt
//! beroende" (se ROADMAP.md för den fulla designmotiveringen): Bastion
//! ska kunna ladda ner+köra dessa verktyg själv istället för att kräva
//! att användaren installerat dem separat, men en nedladdad binär är
//! samma tillitsnivå som ett `curl | sudo bash`-skript om checksumman
//! inte verifieras. Detta är MEDVETET framåtblickande infrastruktur —
//! "första byggstenen" (samma fras Swift-sidans kommentar använder), inte
//! en stängd lucka. Medvetet GENERISK (URL + förväntad checksumma in,
//! verifierad sökväg ut) — varken WireGuard- eller Tailscale-specifik,
//! så samma funktion återanvänds för båda (och andra framtida verktyg)
//! utan duplicering.
//!
//! `#![allow(dead_code)]`: hela modulen är medvetet INTE kopplad in i
//! `main.rs` än — den väntar på att en faktisk WireGuard-/Tailscale-
//! nedladdningsfunktion (ett framtida UI-lager) anropar den, precis som
//! Swift-referensen själv saknar en anropare. Bara testad, inte använd,
//! samma motivering som `WireGuardProfileStore::get` i `wireguard.rs`.
#![allow(dead_code)]

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq)]
pub enum ExternalBinaryError {
    DownloadFailed(String),
    ChecksumMismatch { expected: String, actual: String },
    CacheWriteFailed(String),
    /// Arkivet gick att läsa men innehöll inte filen vi bad om. Eget fall
    /// i stället för `DownloadFailed`: nedladdningen lyckades och
    /// checksumman stämde, så felet ligger i vad vi LETADE efter — ett
    /// namn som ändrats mellan versioner, inte ett nätverksproblem.
    EntryNotFound(String),
    ExtractFailed(String),
}

impl std::fmt::Display for ExternalBinaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExternalBinaryError::DownloadFailed(e) => write!(f, "nedladdningen misslyckades: {e}"),
            ExternalBinaryError::EntryNotFound(name) => {
                write!(f, "arkivet innehöll ingen fil som heter {name:?}")
            }
            ExternalBinaryError::ExtractFailed(e) => write!(f, "uppackningen misslyckades: {e}"),
            ExternalBinaryError::ChecksumMismatch { expected, actual } => {
                write!(f, "fel checksumma: förväntade {expected}, fick {actual}")
            }
            ExternalBinaryError::CacheWriteFailed(e) => write!(f, "kunde inte skriva till cachen: {e}"),
        }
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// Hämtar `url` till `cache_dir/binary_name` om den inte redan finns där
/// med RÄTT checksumma (idempotent — ett andra anrop med samma parametrar
/// gör ingen nätverkstrafik alls). En redan cachad fil med FEL checksumma
/// (korrupt/manipulerad) tas bort och laddas ner på nytt, aldrig litad på
/// tyst.
///
/// Checksumman verifieras mot de NEDLADDADE bytesen INNAN något skrivs
/// till disk — en manipulerad/fel binär hamnar aldrig i cachen ens
/// tillfälligt.
pub async fn fetch(
    client: &reqwest::Client,
    url: &str,
    expected_sha256: &str,
    cache_dir: &std::path::Path,
    binary_name: &str,
) -> Result<std::path::PathBuf, ExternalBinaryError> {
    let destination = cache_dir.join(binary_name);
    let expected = expected_sha256.to_lowercase();

    if let Ok(existing) = std::fs::read(&destination)
        && sha256_hex(&existing) == expected
    {
        return Ok(destination);
    }
    // Finns men med fel checksumma (korrupt eller ett gammalt, felaktigt
    // cachat försök) — städa bort tyst, hämta rent på nytt nedan.
    let _ = std::fs::remove_file(&destination);

    let response = client.get(url).send().await.map_err(|e| ExternalBinaryError::DownloadFailed(e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ExternalBinaryError::DownloadFailed(format!("HTTP {status} för {url}")));
    }
    let data = response.bytes().await.map_err(|e| ExternalBinaryError::DownloadFailed(e.to_string()))?;

    let actual = sha256_hex(&data);
    if actual != expected {
        return Err(ExternalBinaryError::ChecksumMismatch { expected, actual });
    }

    write_executable_atomically(&data, cache_dir, binary_name)?;
    Ok(destination)
}

/// Skriver `data` som en körbar fil på `cache_dir/name`.
///
/// Går via en temporär fil i SAMMA katalog och byter sedan namn: en process
/// som läser målet mitt under en nedladdning ska aldrig kunna se en
/// halvskriven fil, och `rename` inom samma filsystem är atomärt.
fn write_executable_atomically(
    data: &[u8],
    cache_dir: &std::path::Path,
    name: &str,
) -> Result<std::path::PathBuf, ExternalBinaryError> {
    std::fs::create_dir_all(cache_dir).map_err(|e| ExternalBinaryError::CacheWriteFailed(e.to_string()))?;
    let destination = cache_dir.join(name);
    let tmp = cache_dir.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, data).map_err(|e| ExternalBinaryError::CacheWriteFailed(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp).map_err(|e| ExternalBinaryError::CacheWriteFailed(e.to_string()))?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms).map_err(|e| ExternalBinaryError::CacheWriteFailed(e.to_string()))?;
    }
    let _ = std::fs::remove_file(&destination);
    std::fs::rename(&tmp, &destination).map_err(|e| ExternalBinaryError::CacheWriteFailed(e.to_string()))?;
    Ok(destination)
}

/// Packar upp EN namngiven fil ur ett `.tar.gz` och skriver den körbar i
/// `cache_dir`.
///
/// Behövs för att `fetch` ensam inte räcker för alla verktyg: Tailscale
/// levererar en `.tgz` med binärerna i en versionsnamngiven mapp
/// (`tailscale_1.102.2_amd64/tailscale`), inte en naken binär. Utan det här
/// steget skulle cachen innehålla en körbarhetsmarkerad tarball.
///
/// # Sökvägssäkerhet
///
/// Bara SISTA komponenten i varje arkiventry jämförs, och bara den
/// efterfrågade filen skrivs — till en sökväg vi själva bygger. Ett entry
/// som heter `../../.ssh/authorized_keys` eller `/etc/passwd` kan därför
/// aldrig styra vart något hamnar, oavsett vad arkivet påstår. Det är ett
/// medvetet val framför `Archive::unpack`, som packar upp ALLT och vars
/// skydd man får lita på i stället för att äga.
pub fn extract_file_from_tar_gz(
    archive: &[u8],
    wanted_file_name: &str,
    cache_dir: &std::path::Path,
) -> Result<std::path::PathBuf, ExternalBinaryError> {
    use std::io::Read as _;

    let decoder = flate2::read::GzDecoder::new(archive);
    let mut tar = tar::Archive::new(decoder);
    let entries = tar
        .entries()
        .map_err(|e| ExternalBinaryError::ExtractFailed(e.to_string()))?;

    for entry in entries {
        let mut entry = entry.map_err(|e| ExternalBinaryError::ExtractFailed(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| ExternalBinaryError::ExtractFailed(e.to_string()))?
            .into_owned();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name != wanted_file_name {
            continue;
        }
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|e| ExternalBinaryError::ExtractFailed(e.to_string()))?;
        return write_executable_atomically(&data, cache_dir, wanted_file_name);
    }

    Err(ExternalBinaryError::EntryNotFound(wanted_file_name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Riktiga nätverksanrop mot en pinnad, oföränderlig GitHub-tagg (inte
    // ett mockat svar) — samma rigör som repots övriga "verifierat mot
    // RIKTIGT X"-tester, och samma URL/checksumma som Swift-sidans
    // `ExternalBinaryFetcherTests`. Checksumman verifierades separat
    // (`curl` + `sha256sum`) innan testet skrevs — testet bevisar alltså
    // att fetcher-koden känner igen en korrekt checksumma, inte bara att
    // den accepterar vad den själv laddade ner.
    const SAMPLE_URL: &str = "https://raw.githubusercontent.com/torvalds/linux/v6.6/COPYING";
    const SAMPLE_SHA256: &str = "fb5a425bd3b3cd6071a3a9aff9909a859e7c1158d54d32e07658398cd67eb6a0";

    fn fresh_cache_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("bastion-binfetch-test-{}", uuid::Uuid::new_v4()))
    }

    /// Nätverksberoende — hoppa tydligt över (som `TestSshd`/övriga
    /// miljöberoende tester i den här kodbasen) istället för att låta ett
    /// sandboxat/offline testläge misslyckas förvirrande.
    async fn network_available(client: &reqwest::Client) -> bool {
        client
            .head(SAMPLE_URL)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn downloads_and_verifies_a_real_file() {
        let client = reqwest::Client::new();
        if !network_available(&client).await {
            eprintln!("hoppar: ingen nätverksåtkomst i den här miljön");
            return;
        }
        let cache_dir = fresh_cache_dir();

        let path = fetch(&client, SAMPLE_URL, SAMPLE_SHA256, &cache_dir, "sample").await.expect("fetch misslyckades");

        assert!(path.exists());
        let data = std::fs::read(&path).unwrap();
        assert_eq!(sha256_hex(&data), SAMPLE_SHA256);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "binären ska vara körbar (chmod 755)");
        }
        std::fs::remove_dir_all(cache_dir).ok();
    }

    #[tokio::test]
    async fn second_fetch_is_a_cache_hit_and_skips_the_network() {
        let client = reqwest::Client::new();
        if !network_available(&client).await {
            eprintln!("hoppar: ingen nätverksåtkomst i den här miljön");
            return;
        }
        let cache_dir = fresh_cache_dir();

        let first = fetch(&client, SAMPLE_URL, SAMPLE_SHA256, &cache_dir, "sample").await.expect("fetch misslyckades");

        // En URL som INTE går att nå — om detta andra anrop av misstag
        // gjorde ett nätverksanrop skulle det faila, inte returnera tyst.
        // Bevisar att cache-träffen faktiskt undviker nätverket, inte
        // bara att resultatet råkar stämma.
        let unreachable = "https://127.0.0.1.invalid/does-not-exist";
        let second = fetch(&client, unreachable, SAMPLE_SHA256, &cache_dir, "sample").await.expect("cache-träffen skulle inte gjort något nätverksanrop alls");

        assert_eq!(first, second);
        std::fs::remove_dir_all(cache_dir).ok();
    }

    #[tokio::test]
    async fn wrong_checksum_is_rejected_and_never_cached() {
        let client = reqwest::Client::new();
        if !network_available(&client).await {
            eprintln!("hoppar: ingen nätverksåtkomst i den här miljön");
            return;
        }
        let cache_dir = fresh_cache_dir();
        let wrong_checksum = "0".repeat(64);

        let err = fetch(&client, SAMPLE_URL, &wrong_checksum, &cache_dir, "sample").await.expect_err("förväntade ChecksumMismatch");
        match err {
            ExternalBinaryError::ChecksumMismatch { expected, actual } => {
                assert_eq!(expected, wrong_checksum);
                assert_eq!(actual, SAMPLE_SHA256);
            }
            other => panic!("fel feltyp: {other:?}"),
        }

        // Den felaktiga nedladdningen ska ALDRIG ha skrivits till disk.
        assert!(!cache_dir.join("sample").exists());
        std::fs::remove_dir_all(cache_dir).ok();
    }

    #[tokio::test]
    async fn a_corrupted_cache_entry_is_redownloaded() {
        let client = reqwest::Client::new();
        if !network_available(&client).await {
            eprintln!("hoppar: ingen nätverksåtkomst i den här miljön");
            return;
        }
        let cache_dir = fresh_cache_dir();
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("sample"), "korrupt-skräp, inte den riktiga filen").unwrap();

        let path = fetch(&client, SAMPLE_URL, SAMPLE_SHA256, &cache_dir, "sample").await.expect("fetch misslyckades");

        let data = std::fs::read(&path).unwrap();
        assert_eq!(sha256_hex(&data), SAMPLE_SHA256);
        std::fs::remove_dir_all(cache_dir).ok();
    }

    /// Bygger ett riktigt `.tar.gz` i minnet, med samma form Tailscale
    /// använder: binärerna ligger i en versionsnamngiven mapp, inte i roten.
    fn tar_gz_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, *data).unwrap();
        }
        let tar_bytes = builder.into_inner().unwrap();
        use std::io::Write as _;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar_bytes).unwrap();
        gz.finish().unwrap()
    }

    /// Grundfallet: rätt fil ur en versionsnamngiven mapp, körbar efteråt.
    /// Utan uppackningssteget skulle cachen innehålla en körbarhetsmarkerad
    /// tarball, vilket ser ut att ha lyckats ända tills något försöker köra
    /// den.
    #[test]
    fn the_wanted_file_is_extracted_from_inside_a_versioned_directory() {
        let archive = tar_gz_with(&[
            ("tailscale_1.102.2_amd64/tailscale", b"BINAR-INNEHALL"),
            ("tailscale_1.102.2_amd64/tailscaled", b"ANNAT"),
        ]);
        let dir = fresh_cache_dir();
        let path = extract_file_from_tar_gz(&archive, "tailscale", &dir).unwrap();

        assert_eq!(path, dir.join("tailscale"));
        assert_eq!(std::fs::read(&path).unwrap(), b"BINAR-INNEHALL");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755, "en uppackad binär måste vara körbar");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Bygger tar-headern FÖR HAND, 512 byte enligt ustar-formatet.
    ///
    /// `tar::Builder` vägrar skriva ett entry vars sökväg innehåller `..` —
    /// vilket är bra av den, men gör den oanvändbar för att BYGGA det
    /// skadliga arkiv vi måste försvara oss mot. Ett angripares arkiv går
    /// inte genom vår builder. Utan den här funktionen skulle testet nedan
    /// bara bevisa att `tar`-crate:n är säker, inte att vår kod är det.
    fn tar_gz_with_raw_path(path: &str, data: &[u8]) -> Vec<u8> {
        let mut header = [0u8; 512];
        let name = path.as_bytes();
        header[..name.len()].copy_from_slice(name);
        header[100..107].copy_from_slice(b"0000644");           // mode
        header[108..115].copy_from_slice(b"0000000");           // uid
        header[116..123].copy_from_slice(b"0000000");           // gid
        let size = format!("{:011o}", data.len());
        header[124..135].copy_from_slice(size.as_bytes());      // size
        header[136..147].copy_from_slice(b"00000000000");       // mtime
        header[156] = b'0';                                     // typeflag: vanlig fil
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        // Checksumman räknas med checksummefältet fyllt av blanksteg.
        header[148..156].copy_from_slice(b"        ");
        let sum: u32 = header.iter().map(|&b| b as u32).sum();
        let sum_field = format!("{sum:06o}\0 ");
        header[148..156].copy_from_slice(sum_field.as_bytes());

        let mut tar_bytes = header.to_vec();
        tar_bytes.extend_from_slice(data);
        tar_bytes.resize(tar_bytes.len().div_ceil(512) * 512, 0);
        tar_bytes.extend_from_slice(&[0u8; 1024]); // två tomma block = slut

        use std::io::Write as _;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar_bytes).unwrap();
        gz.finish().unwrap()
    }

    /// Ett arkiv kan påstå vad som helst om vart dess filer hör hemma. Bara
    /// sista namnkomponenten används, och målsökvägen byggs av OSS — så ett
    /// entry som pekar utanför cachen kan inte styra vart något hamnar.
    ///
    /// Det här är skälet till att `Archive::unpack` inte används: den packar
    /// upp allt och man får lita på dess skydd i stället för att äga det.
    #[test]
    fn an_entry_that_points_outside_the_cache_cannot_escape_it() {
        let dir = fresh_cache_dir();
        let outside = dir.parent().unwrap().join("bastion-skulle-inte-skrivas");
        std::fs::remove_file(&outside).ok();

        for evil in [
            "../../bastion-skulle-inte-skrivas",
            "/tmp/bastion-skulle-inte-skrivas",
            "mapp/../../bastion-skulle-inte-skrivas",
        ] {
            let archive = tar_gz_with_raw_path(evil, b"OND");
            let path = extract_file_from_tar_gz(&archive, "bastion-skulle-inte-skrivas", &dir)
                .unwrap_or_else(|e| panic!("uppackning av {evil:?} gav {e}"));
            assert_eq!(
                path,
                dir.join("bastion-skulle-inte-skrivas"),
                "filen ska hamna i cachen, inte där arkivet pekade ({evil})"
            );
            assert!(!outside.exists(), "arkivet skrev utanför cachen via {evil}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Ett namn som ändrats mellan versioner är inte ett nätverksfel, och
    /// felet ska säga vad vi letade efter.
    #[test]
    fn a_missing_entry_is_its_own_error_naming_what_was_sought() {
        let archive = tar_gz_with(&[("mapp/tailscaled", b"x")]);
        let dir = fresh_cache_dir();
        match extract_file_from_tar_gz(&archive, "tailscale", &dir) {
            Err(ExternalBinaryError::EntryNotFound(name)) => assert_eq!(name, "tailscale"),
            other => panic!("förväntade EntryNotFound, fick {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_is_not_a_gzip_archive_is_an_error_not_a_panic() {
        let dir = fresh_cache_dir();
        assert!(matches!(
            extract_file_from_tar_gz(b"det har ar inte en tarball", "tailscale", &dir),
            Err(ExternalBinaryError::ExtractFailed(_))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// HELA kedjan mot de skarpa tjänsterna: lös upp utgåvan, hämta dess
    /// publicerade checksumma, ladda ner tarballen, verifiera den och packa
    /// upp den riktiga `tailscale`-binären.
    ///
    /// Varje länk är testad var för sig ovan. Det här beviset att de faktiskt
    /// passar ihop — och att den binär som kommer ut är en riktig körbar fil,
    /// inte en tarball som råkade få körbarhetsbiten satt.
    #[tokio::test]
    #[ignore = "laddar ner ~30 MB från pkgs.tailscale.com"]
    async fn the_whole_chain_produces_a_real_tailscale_binary() {
        use crate::tool_release::{Channel, parse_checksum, tailscale_index_url, tailscale_release};

        let client = reqwest::Client::new();
        let Some(arch) = crate::tool_release::host_arch() else {
            eprintln!("hoppar: Tailscale bygger inte för {}", std::env::consts::ARCH);
            return;
        };
        let index = client.get(tailscale_index_url(Channel::Stable)).send().await
            .expect("nådde inte pkgs.tailscale.com").text().await.unwrap();
        let release = tailscale_release(&index, Channel::Stable, arch).unwrap();

        let sum_body = client.get(&release.checksum_url).send().await.unwrap().text().await.unwrap();
        let expected = parse_checksum(&sum_body).unwrap();

        let dir = fresh_cache_dir();
        let tarball = fetch(&client, &release.download_url, &expected, &dir, "tailscale.tgz")
            .await
            .expect("nedladdning + checksummekontroll misslyckades");

        let data = std::fs::read(&tarball).unwrap();
        let binary = extract_file_from_tar_gz(&data, "tailscale", &dir).unwrap();

        // ELF-magin bevisar att det är en binär, inte en tarball.
        let head = std::fs::read(&binary).unwrap();
        assert_eq!(&head[..4], b"\x7fELF", "det uppackade ska vara en riktig körbar fil");
        std::fs::remove_dir_all(&dir).ok();
    }
}
