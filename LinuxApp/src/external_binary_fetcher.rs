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
}

impl std::fmt::Display for ExternalBinaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExternalBinaryError::DownloadFailed(e) => write!(f, "nedladdningen misslyckades: {e}"),
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

    std::fs::create_dir_all(cache_dir).map_err(|e| ExternalBinaryError::CacheWriteFailed(e.to_string()))?;
    // Skriv till en temporär fil i SAMMA katalog, byt sedan namn — en
    // process som läser `destination` mitt under en nedladdning ska
    // aldrig kunna se en halvskriven fil.
    let tmp = cache_dir.join(format!(".{binary_name}.{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, &data).map_err(|e| ExternalBinaryError::CacheWriteFailed(e.to_string()))?;
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
}
