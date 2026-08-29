//! Hämtar ett värdlösenord ur en LOKALT installerad Bitwarden CLI (`bw`) —
//! gratis kärnfunktion, inget betalt tillägg krävs. Port av
//! `Sources/SSHCore/BitwardenClient.swift`.
//!
//! **Viktigt, varför detta är MEST värdefullt just här**: Swift-sidans
//! `App/AuthResolver.swift` returnerar ALLTID `nil` för
//! `HostAuth::BitwardenItem`, på BÅDA Apple-plattformarna — iOS saknar
//! `Foundation.Process` helt, och macOS-målets App Sandbox
//! (`com.apple.security.app-sandbox`) dödar processen med ett okatchbart
//! SIGTRAP så fort den försöker starta den osignerade `bw`-binären
//! (empiriskt verifierat på riktig macOS-hårdvara, se `AuthResolver.swift`s
//! kommentar). Bastions egen kod förutsätter uttryckligen att `bw` "faktiskt
//! fungerar" på just Linux-sidan (samma kommentar) — LinuxApp är alltså inte
//! bara paritet med en redan fungerande Apple-funktion, det är den ENDA
//! plattform där `HostAuth::BitwardenItem` någonsin kan fungera i praktiken.
//!
//! Bastion loggar aldrig in/låser upp valvet självt — förutsätter att
//! användaren redan kört `bw login` och har en giltig sessionsnyckel.

#[derive(Debug, Clone, PartialEq)]
pub enum BitwardenError {
    Io(String),
    CommandFailed { exit_code: i32, stderr: String },
    EmptyPassword,
}

impl std::fmt::Display for BitwardenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BitwardenError::Io(e) => write!(f, "kunde inte starta bw: {e}"),
            BitwardenError::CommandFailed { exit_code, stderr } => {
                write!(f, "bw avslutade med kod {exit_code}: {stderr}")
            }
            BitwardenError::EmptyPassword => write!(f, "bw gav ett tomt lösenord"),
        }
    }
}

/// `session`: skickas via miljövariabeln `BW_SESSION` (INTE som argv
/// `--session`, vilket läcker via `/proc/*/cmdline`) om satt, annars faller
/// `bw` tillbaka på egen sessionscache/miljö.
///
/// `--nointeraction`: utan sessionsnyckel/med en utgången session skulle
/// `bw` annars fråga interaktivt efter huvudlösenordet — en process startad
/// från Bastion har ingen terminal att fråga i, så det hade bara hängt tills
/// en anropande timeout gett upp, i stället för att faila direkt.
pub fn fetch_password(program: &str, item_id: &str, session: Option<&str>) -> Result<String, BitwardenError> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(["get", "password", item_id, "--nointeraction"]);
    if let Some(session) = session
        && !session.is_empty()
    {
        cmd.env("BW_SESSION", session);
    }
    let output = cmd.output().map_err(|e| BitwardenError::Io(e.to_string()))?;
    if !output.status.success() {
        return Err(BitwardenError::CommandFailed {
            exit_code: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    // INTE trimmat rakt av — bara den avslutande radbrytningen `bw` själv
    // lägger till på stdout, aldrig inre whitespace (ett riktigt lösenord
    // kan avsiktligt innehålla ledande/efterföljande blanksteg).
    let mut password = String::from_utf8_lossy(&output.stdout).into_owned();
    while password.ends_with('\n') || password.ends_with('\r') {
        password.pop();
    }
    if password.is_empty() {
        return Err(BitwardenError::EmptyPassword);
    }
    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Skyddar mot en genuin (om än obskyr) `fork()`-kapplöpning: `cargo
    /// test` kör alla den här filens tester som separata OS-trådar i SAMMA
    /// process, och `std::process::Command::output()` forkar internt.
    /// Forkar tråd B medan tråd A fortfarande har SITT skriptfilshandtag
    /// öppet för skrivning (innan `drop(f)` hunnit köra), ärver barnet en
    /// kopia av det handtaget — kärnan kan då tillfälligt neka `execve()`
    /// med `ETXTBSY` ("Text file busy") även om barnet exekverar ett HELT
    /// ANNAT (redan färdigskrivet) skript. Ren testinfrastruktur-flakighet,
    /// inte ett fel i `fetch_password` (bevisat: 100 % grönt med
    /// `--test-threads=1`) — förekommer även mot HELA binärens övriga
    /// tester (t.ex. `ssh.rs`s `TestSshd`, som också forkar), inte bara
    /// mellan den här filens egna tester, så låset räcker inte ensamt;
    /// `run_fixture` nedan lägger till en kort ETXTBSY-specifik
    /// omförsöksloop ovanpå.
    static SCRIPT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Skriver ett riktigt, körbart `/bin/sh`-skript — samma mönster som
    /// `TestSshd` och Swift-sidans `BitwardenClientTests.makeScript`, en
    /// verklig kortlivad process istället för en mockad `bw`.
    fn make_script(body: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("bastion-bw-fixture-{}.sh", uuid::Uuid::new_v4()));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        f.sync_all().unwrap();
        drop(f);
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o700);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    /// Kör `make_script` + `fetch_password` under `SCRIPT_LOCK`, med en
    /// kort omförsöksloop specifikt för `ETXTBSY` (se `SCRIPT_LOCK`s
    /// dokumentation — en genuin men ren testfixtur-artefakt, aldrig ett
    /// verkligt fel `fetch_password`s anropare behöver hantera, så
    /// omförsöket hör hemma här, inte i produktionskoden). Returnerar
    /// samma `Result` som `fetch_password`.
    fn run_fixture(body: &str, item_id: &str, session: Option<&str>) -> (std::path::PathBuf, Result<String, BitwardenError>) {
        let _guard = SCRIPT_LOCK.lock().unwrap();
        let script = make_script(body);
        let mut result = fetch_password(script.to_str().unwrap(), item_id, session);
        for _ in 0..20 {
            let is_etxtbsy = matches!(&result, Err(BitwardenError::Io(msg)) if msg.contains("os error 26"));
            if !is_etxtbsy {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            result = fetch_password(script.to_str().unwrap(), item_id, session);
        }
        (script, result)
    }

    #[test]
    fn fetch_password_strips_trailing_newline() {
        let (script, result) = run_fixture("#!/bin/sh\nprintf 'hunter2\\n'\n", "irrelevant", None);
        assert_eq!(result.unwrap(), "hunter2");
        std::fs::remove_file(script).ok();
    }

    /// Ledande/inre whitespace i ett RIKTIGT lösenord får INTE trimmas bort.
    #[test]
    fn fetch_password_preserves_inner_whitespace_and_leading_space() {
        let (script, result) = run_fixture("#!/bin/sh\nprintf ' a b \\n'\n", "irrelevant", None);
        assert_eq!(result.unwrap(), " a b ");
        std::fs::remove_file(script).ok();
    }

    #[test]
    fn fetch_password_passes_session_via_environment() {
        // Ekar BW_SESSION-miljövariabeln så testet kan verifiera att
        // sessionen skickas som miljö (INTE som argv `--session`).
        let (script, result) = run_fixture("#!/bin/sh\necho \"$BW_SESSION\"\n", "my-item", Some("tok123"));
        assert_eq!(result.unwrap(), "tok123");
        std::fs::remove_file(script).ok();
    }

    /// Regressionsskydd för `--nointeraction`: utan flaggan hade `bw`
    /// kunnat hänga och vänta på ett interaktivt huvudlösenords-prompt
    /// (Bastion har ingen terminal att fråga i) i stället för att faila
    /// direkt.
    #[test]
    fn fetch_password_passes_nointeraction_flag() {
        let (script, result) = run_fixture("#!/bin/sh\necho \"$@\"\n", "my-item", None);
        let output = result.unwrap();
        assert!(output.contains("--nointeraction"), "argv saknade --nointeraction");
        std::fs::remove_file(script).ok();
    }

    #[test]
    fn fetch_password_fails_on_non_zero_exit() {
        let (script, result) = run_fixture("#!/bin/sh\necho 'Vault is locked.' >&2\nexit 1\n", "irrelevant", None);
        let err = result.unwrap_err();
        match err {
            BitwardenError::CommandFailed { exit_code, stderr } => {
                assert_eq!(exit_code, 1);
                assert!(stderr.contains("Vault is locked"));
            }
            other => panic!("fel feltyp: {other:?}"),
        }
        std::fs::remove_file(script).ok();
    }

    #[test]
    fn fetch_password_fails_on_empty_output() {
        let (script, result) = run_fixture("#!/bin/sh\nprintf ''\n", "irrelevant", None);
        assert_eq!(result.unwrap_err(), BitwardenError::EmptyPassword);
        std::fs::remove_file(script).ok();
    }
}
