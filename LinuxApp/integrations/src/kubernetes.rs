//! Kubernetes-integration via `kubectl` över SSH. Andra integrationen
//! bredvid [`docker`](crate::docker), och byggd med samma uppdelning: rena
//! kommandobyggare + parsning här, widgetar i `main.rs`.
//!
//! # Varför `--no-headers` och inte `-o json`
//!
//! JSON vore exaktare, men `kubectl get pods -o json` för ett kluster med
//! några hundra poddar är megabyte av utdata över en SSH-kanal som redan
//! har ett tak (`ssh::run_command` kapar vid 4 MiB). `--no-headers` ger
//! samma fält som `kubectl` självt visar, på en rad per objekt, och det
//! är dessutom formatet användaren känner igen från sin egen terminal.
//!
//! Fälten separeras av MELLANSLAG, inte av ett valt tecken som Dockers
//! `|`. Det går bra ändå: inget av fälten (namn, status, antal) kan
//! innehålla mellanslag — Kubernetes-namn är RFC 1123-etiketter.
//!
//! # Namnrymder
//!
//! Den stora strukturella skillnaden mot Docker. Nästan varje objekt bor
//! i en namnrymd, och att utelämna den ger tyst `default` — vilket är fel
//! kluster-vy för de flesta. Varje kommando tar därför en explicit
//! [`Namespace`], och "alla namnrymder" är ett eget alternativ snarare än
//! ett tomt värde som betyder något annat.

/// Vilken namnrymd ett kommando gäller.
///
/// Egen typ i stället för `Option<String>`: `None` skulle kunna läsas
/// som antingen "default" eller "alla", och de två är motsatser. Att
/// tvinga fram valet gör felet omöjligt.
#[derive(Debug, Clone, PartialEq)]
pub enum Namespace {
    All,
    Named(String),
}

impl Namespace {
    /// Flaggan som ska in i kommandot.
    fn flag(&self) -> Result<String, String> {
        match self {
            Namespace::All => Ok("--all-namespaces".to_string()),
            Namespace::Named(name) => Ok(format!("-n {}", validate(name)?)),
        }
    }
}

/// Kubernetes-namn är RFC 1123-etiketter: gemener, siffror och
/// bindestreck, måste börja och sluta alfanumeriskt, max 63 tecken.
///
/// Regeln är avsiktligt SNÄVARE än Dockers motsvarighet — inga versaler,
/// inga punkter, inga understreck. Det är inte överdriven försiktighet
/// utan vad API-servern faktiskt accepterar; ett namn som inte matchar
/// finns inte att hämta ändå.
///
/// Att den är snäv gör den också till ett fullgott injektionsskydd:
/// mellanslag, `;`, `$`, `` ` `` och citattecken faller alla utanför.
pub fn validate(name: &str) -> Result<&str, String> {
    let ok = !name.is_empty()
        && name.len() <= 63
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.ends_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(name)
    } else {
        Err(format!("ogiltigt Kubernetes-namn: {name:?}"))
    }
}

/// En podd.
#[derive(Debug, Clone, PartialEq)]
pub struct Pod {
    pub namespace: String,
    pub name: String,
    /// `2/3` — klara containrar av totalt. Lämnas som text: det är två
    /// tal med en betydelse ihop, och att dela dem gör raden svårare att
    /// läsa, inte lättare.
    pub ready: String,
    pub status: String,
    pub restarts: String,
}

impl Pod {
    /// Kör podden som den ska?
    ///
    /// `Running` räcker INTE. En podd kan stå i `Running` med 1/3 klara
    /// containrar i timmar — det är precis det läget man letar efter när
    /// något är fel, och att måla den grön vore att dölja felet.
    /// `Completed` är däremot friskt: ett jobb som gjort sitt.
    pub fn is_healthy(&self) -> bool {
        match self.status.as_str() {
            "Completed" | "Succeeded" => true,
            "Running" => self.ready_matches(),
            _ => false,
        }
    }

    /// Är alla containrar i podden klara? `2/3` → falskt, `3/3` → sant.
    fn ready_matches(&self) -> bool {
        match self.ready.split_once('/') {
            Some((have, want)) => !have.is_empty() && have == want,
            None => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Deployment {
    pub namespace: String,
    pub name: String,
    /// `3/3` — tillgängliga repliker av önskade.
    pub ready: String,
}

impl Deployment {
    pub fn is_fully_available(&self) -> bool {
        match self.ready.split_once('/') {
            Some((have, want)) => !have.is_empty() && have == want,
            None => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    /// `Ready`, `NotReady`, eller `Ready,SchedulingDisabled`.
    pub status: String,
    pub version: String,
}

impl Node {
    pub fn is_ready(&self) -> bool {
        // Delsträngsmatchning, inte likhet: en avstängd men frisk nod
        // rapporteras som `Ready,SchedulingDisabled`.
        self.status.split(',').any(|part| part == "Ready")
    }

    /// Är noden avstängd för schemaläggning (`kubectl cordon`)?
    pub fn is_cordoned(&self) -> bool {
        self.status.split(',').any(|part| part == "SchedulingDisabled")
    }
}

pub fn pods_command(namespace: &Namespace) -> Result<String, String> {
    Ok(format!("kubectl get pods {} --no-headers 2>/dev/null", namespace.flag()?))
}

pub fn deployments_command(namespace: &Namespace) -> Result<String, String> {
    Ok(format!("kubectl get deployments {} --no-headers 2>/dev/null", namespace.flag()?))
}

/// Noder är kluster-globala och tar därför INGEN namnrymd.
pub fn nodes_command() -> String {
    "kubectl get nodes --no-headers 2>/dev/null".to_string()
}

pub fn namespaces_command() -> String {
    "kubectl get namespaces --no-headers 2>/dev/null".to_string()
}

pub fn pod_logs_command(namespace: &str, pod: &str, tail: i64) -> Result<String, String> {
    let n = tail.max(1);
    Ok(format!(
        "kubectl -n {} logs --tail {n} {} 2>&1",
        validate(namespace)?,
        validate(pod)?
    ))
}

/// Beskriver ett objekt — händelser, villkor och orsaken till att något
/// inte startar. Den vanligaste felsökningsvägen i Kubernetes.
pub fn describe_pod_command(namespace: &str, pod: &str) -> Result<String, String> {
    Ok(format!(
        "kubectl -n {} describe pod {} 2>&1",
        validate(namespace)?,
        validate(pod)?
    ))
}

/// Tar bort en podd så att dess controller skapar en ny.
///
/// Motsvarar Dockers "starta om" i praktiken, men heter `delete` för att
/// det är vad som faktiskt händer — en podd startas aldrig om, den
/// ersätts. Namnet spelar roll: en podd UTAN controller (en lös
/// `kubectl run`) kommer inte tillbaka, och den som tror att knappen
/// betyder "restart" blir förvånad.
pub fn delete_pod_command(namespace: &str, pod: &str) -> Result<String, String> {
    Ok(format!(
        "kubectl -n {} delete pod {} 2>&1",
        validate(namespace)?,
        validate(pod)?
    ))
}

/// Rullande omstart av en deployment — ersätter poddarna en i taget utan
/// avbrott, till skillnad från att radera dem.
pub fn restart_deployment_command(namespace: &str, deployment: &str) -> Result<String, String> {
    Ok(format!(
        "kubectl -n {} rollout restart deployment/{} 2>&1",
        validate(namespace)?,
        validate(deployment)?
    ))
}

/// Delar en `--no-headers`-rad på mellanslag.
///
/// `split_whitespace` och inte `split(' ')`: kubectl kolumnjusterar med
/// varierande antal mellanslag, så ett enda blanksteg som separator ger
/// en drös tomma fält.
fn fields(line: &str, expected: usize) -> Option<Vec<&str>> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < expected {
        return None;
    }
    Some(parts)
}

/// `kubectl get pods` ger NAME READY STATUS RESTARTS AGE, och med
/// `--all-namespaces` en NAMESPACE-kolumn FÖRST. Vilket format som gäller
/// beror alltså på hur frågan ställdes — därför skickas namnrymden med in
/// i stället för att gissas ur radens fältantal (en podd vars namn
/// råkar se ut som en namnrymd hade förstört den gissningen).
pub fn parse_pods(output: &str, namespace: &Namespace) -> Vec<Pod> {
    let all = matches!(namespace, Namespace::All);
    let needed = if all { 5 } else { 4 };
    output
        .lines()
        .filter_map(|line| {
            let f = fields(line, needed)?;
            let (ns, rest) = if all {
                (f[0].to_string(), &f[1..])
            } else {
                match namespace {
                    Namespace::Named(n) => (n.clone(), &f[..]),
                    Namespace::All => unreachable!(),
                }
            };
            Some(Pod {
                namespace: ns,
                name: rest[0].to_string(),
                ready: rest[1].to_string(),
                status: rest[2].to_string(),
                restarts: rest[3].to_string(),
            })
        })
        .collect()
}

/// `kubectl get deployments` ger NAME READY UP-TO-DATE AVAILABLE AGE.
pub fn parse_deployments(output: &str, namespace: &Namespace) -> Vec<Deployment> {
    let all = matches!(namespace, Namespace::All);
    let needed = if all { 3 } else { 2 };
    output
        .lines()
        .filter_map(|line| {
            let f = fields(line, needed)?;
            let (ns, rest) = if all {
                (f[0].to_string(), &f[1..])
            } else {
                match namespace {
                    Namespace::Named(n) => (n.clone(), &f[..]),
                    Namespace::All => unreachable!(),
                }
            };
            Some(Deployment {
                namespace: ns,
                name: rest[0].to_string(),
                ready: rest[1].to_string(),
            })
        })
        .collect()
}

/// `kubectl get nodes` ger NAME STATUS ROLES AGE VERSION.
pub fn parse_nodes(output: &str) -> Vec<Node> {
    output
        .lines()
        .filter_map(|line| {
            let f = fields(line, 5)?;
            Some(Node {
                name: f[0].to_string(),
                status: f[1].to_string(),
                // Versionen är sista kolumnen. ROLES kan innehålla flera
                // kommaseparerade värden men aldrig mellanslag, så
                // positionen bakifrån är stabil.
                version: f[f.len() - 1].to_string(),
            })
        })
        .collect()
}

/// `kubectl get namespaces` ger NAME STATUS AGE.
pub fn parse_namespaces(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| Some(fields(line, 2)?[0].to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ns(name: &str) -> Namespace {
        Namespace::Named(name.to_string())
    }

    /// Regeln är snävare än Dockers med flit — API-servern accepterar
    /// inte versaler, punkter eller understreck, så ett sådant namn finns
    /// inte att hämta ändå.
    #[test]
    fn validate_accepts_rfc1123_labels_and_nothing_else() {
        for good in ["nginx", "my-app-7d9f", "a", "web2", "x9-y"] {
            assert!(validate(good).is_ok(), "{good} skulle accepterats");
        }
        for bad in ["Nginx", "my_app", "app.prod", "-leading", "trailing-", "", &"a".repeat(64)] {
            assert!(validate(bad).is_err(), "{bad:?} skulle avvisats");
        }
    }

    #[test]
    fn injection_cannot_reach_any_command_builder() {
        for bad in ["pod; rm -rf /", "pod && curl evil", "pod$(id)", "pod `id`", "pod|tee x", "'pod'", "a b"] {
            assert!(validate(bad).is_err(), "{bad:?}");
            assert!(pod_logs_command("default", bad, 100).is_err());
            assert!(pod_logs_command(bad, "nginx", 100).is_err());
            assert!(delete_pod_command("default", bad).is_err());
            assert!(restart_deployment_command("default", bad).is_err());
        }
    }

    /// "Alla namnrymder" och "default" är motsatser, och typen tvingar
    /// fram valet i stället för att låta ett tomt värde betyda båda.
    #[test]
    fn namespace_all_and_named_produce_different_flags() {
        assert_eq!(pods_command(&Namespace::All).unwrap(),
                   "kubectl get pods --all-namespaces --no-headers 2>/dev/null");
        assert_eq!(pods_command(&ns("kube-system")).unwrap(),
                   "kubectl get pods -n kube-system --no-headers 2>/dev/null");
        assert!(pods_command(&ns("Fel Namn")).is_err());
    }

    /// Kolumnerna skiftar beroende på hur frågan ställdes, så namnrymden
    /// måste följa med in i parsningen — den går inte att gissa ur
    /// fältantalet.
    #[test]
    fn all_namespaces_output_has_an_extra_leading_column() {
        let with_ns = "kube-system  coredns-abc  1/1  Running  0  5d";
        let pods = parse_pods(with_ns, &Namespace::All);
        assert_eq!(pods.len(), 1);
        assert_eq!(pods[0].namespace, "kube-system");
        assert_eq!(pods[0].name, "coredns-abc");
        assert_eq!(pods[0].status, "Running");

        let without = "coredns-abc  1/1  Running  0  5d";
        let pods = parse_pods(without, &ns("kube-system"));
        assert_eq!(pods[0].namespace, "kube-system", "namnrymden kommer från frågan");
        assert_eq!(pods[0].name, "coredns-abc");
    }

    /// Kärnan i hela vyn: en podd i `Running` med 1/3 klara containrar är
    /// INTE frisk. Det är precis det läget man letar efter när något är
    /// fel, och att måla den grön hade dolt felet.
    #[test]
    fn running_with_unready_containers_is_not_healthy() {
        let out = "\
web-1  3/3  Running    0  1d
web-2  1/3  Running    7  1d
job-1  0/1  Completed  0  2h
web-3  0/1  CrashLoopBackOff  12  10m
web-4  0/1  Pending    0  1m";
        let pods = parse_pods(out, &ns("default"));
        assert_eq!(pods.len(), 5);
        let healthy: Vec<&str> = pods.iter().filter(|p| p.is_healthy()).map(|p| p.name.as_str()).collect();
        assert_eq!(healthy, vec!["web-1", "job-1"],
                   "bara fullt klar Running och Completed räknas som friska");
        assert_eq!(pods[1].restarts, "7");
    }

    /// En avstängd nod rapporteras som `Ready,SchedulingDisabled` — den
    /// är frisk OCH avstängd, och likhetsjämförelse hade missat båda.
    #[test]
    fn cordoned_node_is_both_ready_and_disabled() {
        let out = "\
node-a  Ready                      control-plane  30d  v1.31.2
node-b  Ready,SchedulingDisabled   <none>         30d  v1.31.2
node-c  NotReady                   <none>         30d  v1.30.8";
        let nodes = parse_nodes(out);
        assert_eq!(nodes.len(), 3);

        assert!(nodes[0].is_ready() && !nodes[0].is_cordoned());
        assert!(nodes[1].is_ready(), "avstängd är inte samma sak som trasig");
        assert!(nodes[1].is_cordoned());
        assert!(!nodes[2].is_ready());

        // Versionen tas bakifrån eftersom ROLES-kolumnen varierar.
        assert_eq!(nodes[0].version, "v1.31.2");
        assert_eq!(nodes[2].version, "v1.30.8");
    }

    #[test]
    fn deployments_report_partial_availability() {
        let out = "web  2/3  3  2  5d\napi  4/4  4  4  5d";
        let d = parse_deployments(out, &ns("prod"));
        assert_eq!(d.len(), 2);
        assert!(!d[0].is_fully_available(), "2/3 är inte fullt tillgänglig");
        assert!(d[1].is_fully_available());
        assert_eq!(d[0].namespace, "prod");
    }

    /// kubectl kolumnjusterar med varierande antal mellanslag, och tomma
    /// rader ska hoppas över i stället för att bli poster med tomma fält.
    #[test]
    fn ragged_spacing_and_junk_lines_are_handled() {
        assert!(parse_pods("", &ns("default")).is_empty());
        assert!(parse_pods("\n\n", &ns("default")).is_empty());
        assert!(parse_pods("bara-namn", &ns("default")).is_empty(), "för få fält");
        assert!(parse_nodes("node-a  Ready").is_empty(), "nod utan version");

        let pods = parse_pods("\nweb-1     1/1        Running   0     1d\ntrasig\n", &ns("default"));
        assert_eq!(pods.len(), 1);
        assert_eq!(pods[0].name, "web-1");
    }

    /// `delete pod` heter så för att det är vad som händer — en podd
    /// startas aldrig om, den ersätts. En deployment har däremot en
    /// riktig rullande omstart.
    #[test]
    fn pod_is_replaced_while_deployment_rolls() {
        assert_eq!(delete_pod_command("prod", "web-1").unwrap(),
                   "kubectl -n prod delete pod web-1 2>&1");
        assert_eq!(restart_deployment_command("prod", "web").unwrap(),
                   "kubectl -n prod rollout restart deployment/web 2>&1");
        assert_eq!(pod_logs_command("prod", "web-1", 200).unwrap(),
                   "kubectl -n prod logs --tail 200 web-1 2>&1");
        assert!(pod_logs_command("prod", "web-1", 0).unwrap().contains("--tail 1"),
                "noll rader loggar är aldrig vad man menar");
    }

    #[test]
    fn namespaces_are_listed_by_name_only() {
        let out = "default  Active  30d\nkube-system  Active  30d";
        assert_eq!(parse_namespaces(out), vec!["default", "kube-system"]);
        assert!(parse_namespaces("").is_empty());
    }
}
