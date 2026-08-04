//! Delad hjälpare för atomär JSON-/binärfilspersistens. Används av alla
//! `~/.bastion/*.json`-butiker (host.rs/settings.rs/snippet.rs/sync.rs/
//! sync_crypto.rs) — konsoliderat hit efter en CodeRabbit-granskning
//! (PR #216) som pekade på samma icke-atomära `std::fs::write`-mönster på
//! flera ställen: ett krasch/strömavbrott mitt i en direkt skrivning lämnar
//! en trunkerad fil, som `load()`-sidan sedan (innan denna fix) inte kunde
//! skilja från "filen finns inte än" — nästa `upsert`/`push` skrev då
//! tillbaka ett TOMT tillstånd över den trunkerade filen, permanent.

use std::io;
use std::path::Path;

/// Skriver `data` till `path` atomärt: en temporär fil i SAMMA katalog
/// (garanterar att `rename` nedan är en ren atomär filsystemsoperation,
/// inte cross-filesystem-kopiera+radera) skrivs, `fsync`:as, och byts sedan
/// in över målet. Antingen ligger den GAMLA filen kvar orörd eller den NYA
/// helt komplett efter en krasch — aldrig ett trunkerat mellanläge.
pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "sökvägen saknar en förälderkatalog",
            )
        })?;
    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("bastion");
    let tmp_path = dir.join(format!(".{file_name}.tmp.{}", uuid::Uuid::new_v4()));

    let write_result = (|| -> io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(data)?;
        file.sync_all()
    })();
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }
    std::fs::rename(&tmp_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_back_the_exact_bytes() {
        let dir = std::env::temp_dir().join(format!(
            "bastion-atomic-write-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.json");
        atomic_write(&path, b"hej").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hej");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn overwriting_an_existing_file_never_leaves_a_truncated_intermediate_state() {
        let dir = std::env::temp_dir().join(format!(
            "bastion-atomic-write-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.json");
        atomic_write(&path, b"gammalt-innehall-som-ar-langre").unwrap();
        atomic_write(&path, b"nytt").unwrap();
        // Ingen mellanläges-halvskrivning möjlig att observera synkront här,
        // men detta bevisar åtminstone att andra omgången skriver rent och
        // inte lämnar kvar den gamla, längre datan sammanslagen med den nya.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "nytt");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn no_leftover_tmp_file_survives_a_successful_write() {
        let dir = std::env::temp_dir().join(format!(
            "bastion-atomic-write-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.json");
        atomic_write(&path, b"hej").unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporärfilen ska ha bytts bort av rename, inte blivit kvar"
        );
        std::fs::remove_dir_all(dir).ok();
    }
}
