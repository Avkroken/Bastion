//! GitHub via `gh` över SSH. Sjunde och sista integrationen i VISION:s
//! lista.
//!
//! # Varför `gh` på värden och inte GitHubs API härifrån
//!
//! Samma resonemang som för [`crate::cloudflare`], och det är värt att
//! upprepa eftersom frestelsen är större här: `api.github.com` är lätt
//! att prata med, men då hade paketet behövt en HTTP-klient och en token
//! att lagra, och svaret hade handlat om ett konto i stället för om
//! maskinen man är ansluten till.
//!
//! Frågan den här vyn svarar på är en annan: **vad händer med koden på
//! den här servern?** En byggserver, en deploy-värd eller en
//! utvecklingsmaskin har utcheckade repon, och `gh` är redan inloggad
//! där. Det är körningar och PR:er för DET repot, inte för allt man äger.
//!
//! # Arbetskatalogen är en del av frågan
//!
//! `gh` läser repot ur den katalog kommandot körs i. Utan `cd` svarar
//! det om vilket repo som råkar ligga i hemkatalogen, vilket sällan är
//! det man menar. Sökvägen kommer därför in i varje kommando — och
//! eftersom den är godtycklig citeras den, precis som compose-filerna i
//! [`crate::docker`].

use serde_json::Value;

/// En körning i GitHub Actions.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub name: String,
    /// `completed`, `in_progress`, `queued`.
    pub status: String,
    /// `success`, `failure`, `cancelled`, `skipped` — tom medan
    /// körningen pågår.
    pub conclusion: String,
    pub branch: String,
}

impl Run {
    pub fn is_running(&self) -> bool {
        self.status != "completed"
    }

    /// Misslyckades körningen?
    ///
    /// En pågående körning har ingen slutsats, och att läsa tom slutsats
    /// som "inte misslyckad" vore rätt av fel skäl — den kan fortfarande
    /// falla. Därför frågas statusen först.
    pub fn failed(&self) -> bool {
        !self.is_running() && matches!(self.conclusion.as_str(), "failure" | "timed_out" | "startup_failure")
    }
}

/// En pull request.
#[derive(Debug, Clone, PartialEq)]
pub struct PullRequest {
    pub number: i64,
    pub title: String,
    pub is_draft: bool,
    /// `CLEAN`, `BLOCKED`, `BEHIND`, `DIRTY`, `UNKNOWN` …
    pub mergeable: String,
}

impl PullRequest {
    /// Går den att merga just nu?
    ///
    /// `BLOCKED` betyder att en obligatorisk check inte är grön ÄNNU,
    /// vilket är väsensskilt från `DIRTY` (konflikt). Att slå ihop dem
    /// till "kan inte mergas" hade dolt vilken av dem som gäller, och det
    /// är den enda intressanta skillnaden.
    pub fn is_ready(&self) -> bool {
        self.mergeable == "CLEAN" && !self.is_draft
    }
}

/// Citerar en sökväg för ett POSIX-skal.
///
/// Samma regel som `docker::quote_path`: inom enkla citattecken är varje
/// tecken utom `'` självt literalt, så det räcker att avvisa sökvägar som
/// innehåller apostrof. Katalognamn är godtyckliga och kan innehålla
/// mellanslag — en teckenlista hade avvisat giltiga sökvägar.
fn quote_path(path: &str) -> Result<String, String> {
    if path.trim().is_empty() {
        return Err("tom sökväg till repot".to_string());
    }
    if path.contains('\'') {
        return Err(format!(
            "sökvägen innehåller citattecken och går inte att citera säkert: {path:?}"
        ));
    }
    Ok(format!("'{path}'"))
}

/// `cd <repo> && gh <args>`.
///
/// `&&` och inte `;`: misslyckas katalogbytet ska `gh` inte köras alls.
/// Med semikolon hade kommandot i stället svarat om vilket repo som råkar
/// ligga i hemkatalogen — ett svar som ser giltigt ut och gäller fel sak.
fn in_repo(path: &str, args: &str) -> Result<String, String> {
    Ok(format!("cd {} && gh {args} 2>&1", quote_path(path)?))
}

pub fn runs_command(repo_path: &str, limit: u32) -> Result<String, String> {
    let n = limit.clamp(1, 50);
    in_repo(
        repo_path,
        &format!("run list --limit {n} --json name,status,conclusion,headBranch"),
    )
}

pub fn pull_requests_command(repo_path: &str, limit: u32) -> Result<String, String> {
    let n = limit.clamp(1, 50);
    in_repo(
        repo_path,
        &format!("pr list --limit {n} --json number,title,isDraft,mergeStateStatus"),
    )
}

/// Är `gh` installerat och inloggat på värden?
///
/// Två frågor i ett kommando, för de har samma svar när något är fel:
/// utan inloggning svarar allt annat med ett autentiseringsfel som ser
/// ut som tomt resultat.
pub fn auth_status_command() -> String {
    "gh auth status 2>&1".to_string()
}

fn text(item: &Value, key: &str) -> String {
    item.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn array(output: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(output.trim())
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

pub fn parse_runs(output: &str) -> Vec<Run> {
    array(output)
        .iter()
        .filter_map(|item| {
            let name = text(item, "name");
            if name.is_empty() {
                return None;
            }
            Some(Run {
                name,
                status: text(item, "status"),
                conclusion: text(item, "conclusion"),
                branch: text(item, "headBranch"),
            })
        })
        .collect()
}

pub fn parse_pull_requests(output: &str) -> Vec<PullRequest> {
    array(output)
        .iter()
        .filter_map(|item| {
            let number = item.get("number").and_then(Value::as_i64)?;
            Some(PullRequest {
                number,
                title: text(item, "title"),
                is_draft: item.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
                mergeable: text(item, "mergeStateStatus"),
            })
        })
        .collect()
}

/// Tolkar `gh auth status`, som skriver människotext och inte JSON.
///
/// Returnerar `true` när utdatan innehåller den rad `gh` skriver vid
/// lyckad inloggning. Att leta efter FRAMGÅNGSraden och inte efter
/// felord är avsiktligt: felmeddelandena varierar mellan versioner,
/// framgångsraden har varit stabil.
pub fn is_authenticated(output: &str) -> bool {
    output.contains("Logged in to")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `&&` och inte `;` är hela säkerheten i kommandot: misslyckas
    /// katalogbytet ska `gh` inte köras alls. Med semikolon hade svaret
    /// gällt vilket repo som råkar ligga i hemkatalogen — ett svar som
    /// ser giltigt ut men gäller fel sak.
    #[test]
    fn a_failed_cd_must_not_fall_through_to_the_wrong_repo() {
        let cmd = runs_command("/srv/bastion", 5).unwrap();
        assert_eq!(
            cmd,
            "cd '/srv/bastion' && gh run list --limit 5 \
             --json name,status,conclusion,headBranch 2>&1"
        );
        assert!(cmd.contains("&&"));
        assert!(!cmd.contains("; gh"), "semikolon hade kört gh ändå");
    }

    /// Katalognamn är godtyckliga. Mellanslag ska fungera, apostrof är
    /// det enda som bryter citatet och avvisas.
    #[test]
    fn paths_with_spaces_work_and_only_the_quote_breaking_character_is_rejected() {
        assert!(runs_command("/home/anders/mina repon/bastion", 5).is_ok());
        assert!(runs_command("/srv/it's/repo", 5).is_err());
        assert!(runs_command("", 5).is_err());
        assert!(runs_command("   ", 5).is_err());
        // $ och ; är ofarliga INOM citat och ska inte avvisas.
        assert!(runs_command("/srv/repo$test;x", 5).is_ok());
    }

    #[test]
    fn the_limit_is_clamped_to_something_gh_accepts() {
        assert!(runs_command("/r", 0).unwrap().contains("--limit 1"));
        assert!(runs_command("/r", 9999).unwrap().contains("--limit 50"));
        assert!(pull_requests_command("/r", 10).unwrap().contains("--limit 10"));
    }

    /// En pågående körning har ingen slutsats. Att läsa tom slutsats som
    /// "inte misslyckad" vore rätt av fel skäl — den kan fortfarande
    /// falla, så statusen frågas först.
    #[test]
    fn a_running_job_is_not_reported_as_passing() {
        let out = r#"[
          {"name": "CI", "status": "in_progress", "conclusion": "", "headBranch": "main"},
          {"name": "CI", "status": "completed", "conclusion": "failure", "headBranch": "dev"},
          {"name": "CI", "status": "completed", "conclusion": "success", "headBranch": "main"}
        ]"#;
        let runs = parse_runs(out);
        assert_eq!(runs.len(), 3);

        assert!(runs[0].is_running());
        assert!(!runs[0].failed(), "pågående är inte misslyckad");

        assert!(!runs[1].is_running());
        assert!(runs[1].failed());

        assert!(!runs[2].failed());
    }

    #[test]
    fn timeouts_and_startup_failures_count_as_failures() {
        for conclusion in ["timed_out", "startup_failure", "failure"] {
            let out = format!(
                r#"[{{"name": "CI", "status": "completed", "conclusion": "{conclusion}"}}]"#
            );
            assert!(parse_runs(&out)[0].failed(), "{conclusion} ska räknas som fel");
        }
        for conclusion in ["success", "skipped", "cancelled", "neutral"] {
            let out = format!(
                r#"[{{"name": "CI", "status": "completed", "conclusion": "{conclusion}"}}]"#
            );
            assert!(!parse_runs(&out)[0].failed(), "{conclusion} är inte ett fel");
        }
    }

    /// BLOCKED (en check är inte grön ännu) och DIRTY (konflikt) är
    /// väsensskilda, och skillnaden är det enda intressanta. Att slå ihop
    /// dem till "kan inte mergas" hade dolt vilken som gäller.
    #[test]
    fn blocked_and_dirty_are_both_not_ready_but_stay_distinguishable() {
        let out = r#"[
          {"number": 1, "title": "Klar", "isDraft": false, "mergeStateStatus": "CLEAN"},
          {"number": 2, "title": "Väntar på CI", "isDraft": false, "mergeStateStatus": "BLOCKED"},
          {"number": 3, "title": "Konflikt", "isDraft": false, "mergeStateStatus": "DIRTY"},
          {"number": 4, "title": "Utkast", "isDraft": true, "mergeStateStatus": "CLEAN"}
        ]"#;
        let prs = parse_pull_requests(out);
        assert_eq!(prs.len(), 4);

        assert!(prs[0].is_ready());
        assert!(!prs[1].is_ready());
        assert_eq!(prs[1].mergeable, "BLOCKED", "orsaken ska finnas kvar");
        assert!(!prs[2].is_ready());
        assert_eq!(prs[2].mergeable, "DIRTY");
        assert!(!prs[3].is_ready(), "ett utkast är aldrig redo");
    }

    /// Framgångsraden är stabil mellan gh-versioner; felmeddelandena är
    /// det inte. Därför letas det efter den och inte efter felord.
    #[test]
    fn authentication_is_detected_by_the_success_line() {
        assert!(is_authenticated("github.com\n  ✓ Logged in to github.com account blixten85"));
        assert!(!is_authenticated("You are not logged into any GitHub hosts."));
        assert!(!is_authenticated("gh: command not found"));
        assert!(!is_authenticated(""));
    }

    #[test]
    fn non_json_output_yields_nothing() {
        for bad in ["", "   ", "gh: command not found", "{\"inte\": \"array\"}", "null"] {
            assert!(parse_runs(bad).is_empty(), "{bad:?}");
            assert!(parse_pull_requests(bad).is_empty(), "{bad:?}");
        }
        // En PR utan nummer går inte att agera på.
        assert!(parse_pull_requests(r#"[{"title": "utan nummer"}]"#).is_empty());
        assert!(parse_runs(r#"[{"status": "completed"}]"#).is_empty());
    }
}
