//! PKCS#11-tokens (YubiKey, smartkort) via den lokala ssh-agenten.
//!
//! VISION listar PKCS11 och YubiKey under SSH. Mätningen 2026-08-18 visade
//! att halva stödet redan fanns utan att någon visste om det: en nyckel
//! som ligger i ssh-agenten når `HostAuth::AgentDefault`, och
//! `russh`-agentklienten skickar signeringen vidare till agenten. En
//! FIDO2-nyckel (`sk-ssh-ed25519@openssh.com`) passerar därför rakt
//! igenom och touch-prompten sköts av agenten — inget i bastion behövde
//! ändras för det.
//!
//! Det som saknades var vägen IN: ett PKCS#11-token blir inte synligt för
//! agenten av sig självt, utan måste laddas med `ssh-add -s <modul>`. Den
//! här modulen bygger de kommandona och letar upp modulen.
//!
//! # Varför inga citattecken här
//!
//! Till skillnad från `docker`- och `kubernetes`-modulerna, som bygger
//! SHELL-kommandon att skicka över SSH, kör det här mot den LOKALA
//! maskinen via `std::process::Command` — argument för argument, utan
//! något skal emellan. En sökväg med mellanslag eller semikolon är därför
//! ofarlig av konstruktion, inte av validering. Det som ändå kontrolleras
//! är att filen finns och ser ut som ett delat bibliotek, för att felet
//! ska bli begripligt i stället för `ssh-add`s ordknappa utdata.

use std::path::{Path, PathBuf};

/// Sökvägar där PKCS#11-moduler brukar ligga på Linux och macOS.
///
/// Listan är till för att SLIPPA fråga användaren om en sökväg de sällan
/// känner till. Hittas ingenting går det fortfarande att peka ut en modul
/// för hand — automatiken ersätter inte valet, den gissar bara först.
const KNOWN_MODULE_PATHS: &[&str] = &[
    // OpenSC — det vanligaste, täcker de flesta smartkort och YubiKeys
    // PIV-applet.
    "/usr/lib/x86_64-linux-gnu/opensc-pkcs11.so",
    "/usr/lib/aarch64-linux-gnu/opensc-pkcs11.so",
    "/usr/lib64/opensc-pkcs11.so",
    "/usr/lib/opensc-pkcs11.so",
    "/usr/local/lib/opensc-pkcs11.so",
    // Yubicos egen modul, som stöder mer av YubiKeyns funktioner än OpenSC.
    "/usr/lib/x86_64-linux-gnu/libykcs11.so",
    "/usr/lib64/libykcs11.so",
    "/usr/local/lib/libykcs11.so",
    // Homebrew på macOS, båda arkitekturerna.
    "/opt/homebrew/lib/opensc-pkcs11.so",
    "/opt/homebrew/lib/libykcs11.dylib",
    "/usr/local/lib/libykcs11.dylib",
];

/// En hittad PKCS#11-modul.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub path: PathBuf,
    /// Kort namn att visa, härlett ur filnamnet.
    pub label: String,
}

/// Vad ett filnamn säger om vilken modul det är.
///
/// Bara filnamnet, inte innehållet: att öppna och inspektera ett delat
/// bibliotek för att sätta en etikett vore mycket arbete för en sträng.
fn label_for(path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.contains("ykcs11") {
        "YubiKey (libykcs11)".to_string()
    } else if name.contains("opensc") {
        "OpenSC (smartkort, YubiKey PIV)".to_string()
    } else {
        name.to_string()
    }
}

/// Ser sökvägen ut som ett delat bibliotek?
///
/// `.so`, `.dylib` eller `.so.N` — det sista för distributioner som
/// versionerar modulen (`opensc-pkcs11.so.0`).
pub fn looks_like_module(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name,
        None => return false,
    };
    name.ends_with(".so")
        || name.ends_with(".dylib")
        || name
            .rsplit_once(".so.")
            .is_some_and(|(_, tail)| !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()))
}

/// Letar upp installerade moduler bland [`KNOWN_MODULE_PATHS`].
///
/// `exists` skickas in i stället för att anropa filsystemet direkt, så att
/// sökningen går att testa utan att bero på vad testmaskinen råkar ha
/// installerat.
pub fn discover_with(exists: impl Fn(&Path) -> bool) -> Vec<Module> {
    let mut found: Vec<Module> = Vec::new();
    for candidate in KNOWN_MODULE_PATHS {
        let path = PathBuf::from(candidate);
        if !exists(&path) {
            continue;
        }
        // Samma modul kan nås via flera sökvägar (symlänkar mellan
        // /usr/lib och /usr/lib64). Etiketten räcker som nyckel: två
        // träffar med samma etikett är samma modul för användarens syfte.
        if found.iter().any(|m| m.label == label_for(&path)) {
            continue;
        }
        found.push(Module { label: label_for(&path), path });
    }
    found
}

pub fn discover() -> Vec<Module> {
    discover_with(|p| p.exists())
}

/// Fel som går att förklara innan `ssh-add` ens körs.
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleError {
    NotFound,
    NotALibrary,
}

impl ModuleError {
    pub fn message(&self, path: &Path) -> String {
        match self {
            ModuleError::NotFound => format!("hittar ingen fil på {}", path.display()),
            ModuleError::NotALibrary => format!(
                "{} ser inte ut som en PKCS#11-modul — den ska vara ett delat \
                 bibliotek (.so eller .dylib)",
                path.display()
            ),
        }
    }
}

/// Kontrollerar modulen innan den skickas till `ssh-add`.
///
/// Poängen är felmeddelandet, inte säkerheten: `ssh-add -s` svarar bara
/// "Could not add card" oavsett om filen saknas, är fel sorts fil eller
/// om tokenet inte sitter i. De två första går att skilja åt här.
pub fn check_with(path: &Path, exists: impl Fn(&Path) -> bool) -> Result<(), ModuleError> {
    if !exists(path) {
        return Err(ModuleError::NotFound);
    }
    if !looks_like_module(path) {
        return Err(ModuleError::NotALibrary);
    }
    Ok(())
}

/// Argumenten till `ssh-add` för att LÄGGA TILL ett token.
///
/// Returnerar argv och inte en kommandosträng — anroparen kör dem med
/// `std::process::Command`, som skickar dem direkt till exec utan skal.
/// Därför behöver sökvägen varken citeras eller saneras.
pub fn add_args(path: &Path) -> Vec<String> {
    vec!["-s".to_string(), path.display().to_string()]
}

/// Argumenten för att TA BORT ett token ur agenten.
pub fn remove_args(path: &Path) -> Vec<String> {
    vec!["-e".to_string(), path.display().to_string()]
}

/// Tolkar `ssh-add`s utdata till något som går att visa.
///
/// `ssh-add -s` skriver "Card added: <modul>" vid lyckat resultat och
/// "Could not add card" vid fel, båda på stderr. Utan den här
/// översättningen ser användaren en tom ruta vid framgång och en kryptisk
/// rad vid fel.
pub fn describe_result(success: bool, output: &str, adding: bool) -> String {
    let trimmed = output.trim();
    if success {
        return if adding {
            "Tokenet är laddat i ssh-agenten. Välj autentisering \"ssh-agent\" på värden \
             för att använda det."
                .to_string()
        } else {
            "Tokenet är borttaget ur ssh-agenten.".to_string()
        };
    }
    // PIN-fel är det överlägset vanligaste, och `ssh-add` säger det rakt
    // ut — då är dess egen text bättre än vår.
    if trimmed.is_empty() {
        "ssh-add misslyckades utan att säga varför. Vanligaste orsakerna: tokenet \
         sitter inte i, eller ingen ssh-agent kör (kontrollera $SSH_AUTH_SOCK)."
            .to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    /// Versionerade moduler är verkliga: Debian levererar
    /// `opensc-pkcs11.so.0`. En regel som bara godtar exakt `.so` hade
    /// avvisat den.
    #[test]
    fn versioned_shared_objects_count_as_modules() {
        for good in [
            "/usr/lib/opensc-pkcs11.so",
            "/usr/lib/opensc-pkcs11.so.0",
            "/usr/lib/libykcs11.so.2",
            "/opt/homebrew/lib/libykcs11.dylib",
        ] {
            assert!(looks_like_module(&p(good)), "{good} skulle godtagits");
        }
        for bad in [
            "/usr/lib/opensc-pkcs11",
            "/etc/ssh/ssh_config",
            "/usr/lib/modul.so.x",
            "/usr/lib/modul.sox",
            "",
        ] {
            assert!(!looks_like_module(&p(bad)), "{bad:?} skulle avvisats");
        }
    }

    /// De två felen går att skilja åt INNAN ssh-add körs, och det är hela
    /// poängen: ssh-add svarar "Could not add card" oavsett vilket.
    #[test]
    fn missing_file_and_wrong_file_type_are_different_errors() {
        let missing = p("/usr/lib/finns-inte.so");
        assert_eq!(check_with(&missing, |_| false), Err(ModuleError::NotFound));
        assert!(ModuleError::NotFound.message(&missing).contains("hittar ingen fil"));

        let wrong = p("/etc/passwd");
        assert_eq!(check_with(&wrong, |_| true), Err(ModuleError::NotALibrary));
        assert!(ModuleError::NotALibrary.message(&wrong).contains("delat"));

        assert_eq!(check_with(&p("/usr/lib/opensc-pkcs11.so"), |_| true), Ok(()));
    }

    /// Samma modul nås ofta via flera sökvägar (symlänk mellan /usr/lib
    /// och /usr/lib64). Användaren ska se den en gång, inte tre.
    #[test]
    fn duplicate_paths_to_the_same_module_are_listed_once() {
        let all_exist = discover_with(|_| true);
        let labels: Vec<&str> = all_exist.iter().map(|m| m.label.as_str()).collect();
        let mut unique = labels.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(labels.len(), unique.len(), "dubbletter i {labels:?}");
        assert!(labels.iter().any(|l| l.contains("OpenSC")));
        assert!(labels.iter().any(|l| l.contains("YubiKey")));
    }

    #[test]
    fn discovery_finds_nothing_when_nothing_is_installed() {
        assert!(discover_with(|_| false).is_empty());
    }

    /// Bara opensc-sökvägen finns → bara OpenSC ska dyka upp. Ett test som
    /// visar att sökningen faktiskt frågar filsystemet och inte listar
    /// tabellen rakt av.
    #[test]
    fn discovery_reports_only_what_actually_exists() {
        let found = discover_with(|path| path.to_string_lossy().contains("ykcs11"));
        assert_eq!(found.len(), 1);
        assert!(found[0].label.contains("YubiKey"));
    }

    /// Argumenten går till exec, inte till ett skal — därför ska sökvägen
    /// följa med ORÖRD, mellanslag och allt. Att citera den här hade
    /// tvärtom brutit den.
    #[test]
    fn paths_are_passed_verbatim_because_there_is_no_shell() {
        let odd = p("/home/anders/mina moduler/opensc-pkcs11.so");
        assert_eq!(
            add_args(&odd),
            vec!["-s".to_string(), "/home/anders/mina moduler/opensc-pkcs11.so".to_string()]
        );
        assert_eq!(remove_args(&odd)[0], "-e");
        assert_eq!(remove_args(&odd)[1], odd.display().to_string());
    }

    /// Vid framgång skriver ssh-add nästan ingenting, så meddelandet måste
    /// komma härifrån — annars ser användaren en tom ruta och vet inte om
    /// det gick vägen.
    #[test]
    fn success_explains_the_next_step_and_failure_keeps_ssh_adds_own_words() {
        let ok = describe_result(true, "", true);
        assert!(ok.contains("ssh-agent"), "ska säga vad som hände: {ok}");
        assert!(ok.contains("autentisering"), "ska säga vad man gör härnäst: {ok}");

        assert!(describe_result(true, "", false).contains("borttaget"));

        // ssh-add vet ofta bäst varför det gick fel — då används dess text.
        let pin = describe_result(false, "  Bad PIN\n", true);
        assert_eq!(pin, "Bad PIN");

        // Men tyst misslyckande måste förklaras, annars står användaren
        // utan ledtråd.
        let silent = describe_result(false, "   \n", true);
        assert!(silent.contains("SSH_AUTH_SOCK"), "tyst fel ska peka på orsakerna: {silent}");
    }
}
