//! Port av Sources/SSHCore/ArchiveOperations.swift. SFTP version 3 har
//! ingen egen arkivsemantik — shellar ut till tar/zip över
//! `ssh::run_command` (samma engångsexec-mönster som Docker-vyn). Sökvägar
//! VALIDERAS inte mot en whitelist (filnamn kan legitimt innehålla
//! mellanslag/unicode) — istället citeras varje sökväg för sig med enkla
//! citattecken (POSIX-shell-säkert).

/// Enkla citattecken runt `s`, med inbäddade `'` eskapade som `'\''`
/// (stänger citatet, ett litterlat-eskapat citattecken, öppnar igen) —
/// standard POSIX-shell-säkert sätt att citera GODTYCKLIG text.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub fn create_tar_gz_command(paths: &[String], archive_name: &str, directory: &str) -> String {
    let quoted_paths = paths.iter().map(|p| shell_quote(p)).collect::<Vec<_>>().join(" ");
    format!("cd {} && tar czf {} -- {}", shell_quote(directory), shell_quote(archive_name), quoted_paths)
}

pub fn extract_tar_gz_command(archive_name: &str, directory: &str) -> String {
    format!("cd {} && tar xzf {}", shell_quote(directory), shell_quote(archive_name))
}

/// `./`-prefix på arkivnamnet + `--` före sökvägarna — `zip` tar
/// arkivnamnet som ett rent positionellt argument, så ett namn/filnamn
/// som börjar med `-` skulle annars tolkas som en flagga (samma
/// CodeRabbit-fynd #125 som redan löstes i Swift-sidan).
/// Testad och klar, men UI:t erbjuder just nu bara tar.gz för "Komprimera"
/// (extract stödjer redan BÅDA formaten) — en enklare förstaversion utan
/// en format-väljardialog.
#[allow(dead_code)]
pub fn create_zip_command(paths: &[String], archive_name: &str, directory: &str) -> String {
    let quoted_paths = paths.iter().map(|p| shell_quote(p)).collect::<Vec<_>>().join(" ");
    let safe_archive_name = format!("./{archive_name}");
    format!("cd {} && zip -r -q {} -- {}", shell_quote(directory), shell_quote(&safe_archive_name), quoted_paths)
}

pub fn extract_zip_command(archive_name: &str, directory: &str) -> String {
    format!("cd {} && unzip -o -q {}", shell_quote(directory), shell_quote(&format!("./{archive_name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_escapes_embedded_single_quotes() {
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("with space"), "'with space'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    /// Bevisar att en filnamn-injektion FAKTISKT nollställs av citeringen —
    /// en RIKTIG shell (`/bin/sh -c`) tolkar hela kommandot som EN sökväg.
    #[test]
    fn shell_quote_survives_real_shell_parsing() {
        let malicious = format!("innocent'; touch /tmp/bastion-injection-proof-{}; echo '", uuid::Uuid::new_v4());
        let quoted = shell_quote(&malicious);
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("printf '%s' {quoted}"))
            .output()
            .expect("kunde inte köra /bin/sh");
        assert_eq!(String::from_utf8_lossy(&output.stdout), malicious);
    }

    #[test]
    fn create_tar_gz_command_matches_reference_implementation() {
        assert_eq!(
            create_tar_gz_command(&["a.txt".into(), "b.txt".into()], "out.tar.gz", "/home/x"),
            "cd '/home/x' && tar czf 'out.tar.gz' -- 'a.txt' 'b.txt'"
        );
    }

    #[test]
    fn extract_tar_gz_command_matches_reference_implementation() {
        assert_eq!(extract_tar_gz_command("out.tar.gz", "/home/x"), "cd '/home/x' && tar xzf 'out.tar.gz'");
    }

    #[test]
    fn create_zip_command_matches_reference_implementation() {
        assert_eq!(
            create_zip_command(&["a.txt".into()], "out.zip", "/home/x"),
            "cd '/home/x' && zip -r -q './out.zip' -- 'a.txt'"
        );
    }

    #[test]
    fn extract_zip_command_matches_reference_implementation() {
        assert_eq!(extract_zip_command("out.zip", "/home/x"), "cd '/home/x' && unzip -o -q './out.zip'");
    }
}
