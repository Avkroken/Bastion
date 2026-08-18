//! WebDAV som synktransport.
//!
//! VISION räknar upp iCloud, Git, GitHub, WebDAV, Dropbox, OneDrive,
//! Syncthing och självhostad server som alternativ. Mapptransporten i
//! [`crate::sync`] täcker Git, Syncthing och allt annat som redan synkar
//! en katalog; WebDAV är den enda i listan som kräver att appen själv
//! talar ett protokoll — och samtidigt den som gör "självhostad server"
//! möjlig utan att något monteras lokalt (Nextcloud, ownCloud, Apache
//! `mod_dav`, `rclone serve webdav`).
//!
//! # Bara GET och PUT
//!
//! WebDAV är stort — PROPFIND, LOCK, MKCOL, XML-svar. Ingenting av det
//! behövs här. Kontraktet i [`crate::sync::SyncProvider`] är "läs ett
//! tillstånd, skriv tillbaka ett" och det är exakt två HTTP-verb mot en
//! bestämd URL. Att implementera resten vore att bygga en filklient som
//! ingen efterfrågat.
//!
//! `404` betyder följaktligen inte fel utan "ingen har synkat än", precis
//! som en saknad fil gör i mapptransporten.
//!
//! # Lösenordet ligger inte här
//!
//! Providern tar färdiga uppgifter och äger inte lagringen av dem. Var de
//! sparas är ett separat beslut (samma som för SSH-lösenord), och att
//! blanda ihop transport och hemlighetshantering gör båda svårare att
//! resonera om.

use crate::host::SyncState;
use crate::sync::SyncProvider;

pub struct WebDavSyncProvider {
    /// Full URL till FILEN, inte till katalogen — `https://moln.example/
    /// remote.php/dav/files/anders/bastion.json`.
    url: String,
    username: String,
    password: String,
}

/// Vad som gick fel, i den detalj som går att åtgärda.
///
/// Ett nätverksfel och ett `401` kräver helt olika saker av användaren,
/// och en enda "synk misslyckades" hade dolt vilken.
#[derive(Debug)]
pub enum WebDavError {
    /// Kunde inte nå servern alls.
    Unreachable(String),
    /// `401`/`403` — fel användarnamn eller lösenord.
    Unauthorized,
    /// Servern svarade, men med något annat än förväntat.
    Status(u16),
    /// Svaret kom fram men gick inte att tolka som ett synktillstånd.
    Malformed(String),
}

impl std::fmt::Display for WebDavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebDavError::Unreachable(e) => write!(f, "kunde inte nå servern: {e}"),
            WebDavError::Unauthorized => {
                write!(f, "servern avvisade inloggningen — kontrollera användarnamn och lösenord")
            }
            WebDavError::Status(code) => write!(f, "servern svarade {code}"),
            WebDavError::Malformed(e) => write!(f, "svaret gick inte att tolka: {e}"),
        }
    }
}

impl From<WebDavError> for std::io::Error {
    fn from(e: WebDavError) -> Self {
        let kind = match e {
            WebDavError::Unauthorized => std::io::ErrorKind::PermissionDenied,
            WebDavError::Unreachable(_) => std::io::ErrorKind::ConnectionRefused,
            WebDavError::Malformed(_) => std::io::ErrorKind::InvalidData,
            WebDavError::Status(_) => std::io::ErrorKind::Other,
        };
        std::io::Error::new(kind, e.to_string())
    }
}

/// Översätter en HTTP-statuskod till utfall.
///
/// Egen funktion, och GTK-fri, eftersom det är här alla intressanta
/// beslut sitter: 404 är inte ett fel, 401 och 403 är samma sak för
/// användaren, och 2xx är det enda som räknas som lyckat.
pub fn classify(status: u16) -> Result<HttpOutcome, WebDavError> {
    match status {
        200..=299 => Ok(HttpOutcome::Ok),
        // Ingen har synkat än. Samma sak som en fil som inte finns i
        // mapptransporten — inte ett fel att rapportera uppåt.
        404 | 410 => Ok(HttpOutcome::Missing),
        401 | 403 => Err(WebDavError::Unauthorized),
        other => Err(WebDavError::Status(other)),
    }
}

#[derive(Debug, PartialEq)]
pub enum HttpOutcome {
    Ok,
    Missing,
}

impl WebDavSyncProvider {
    pub fn new(url: String, username: String, password: String) -> Self {
        WebDavSyncProvider { url, username, password }
    }

    fn client() -> Result<reqwest::Client, WebDavError> {
        reqwest::Client::builder()
            // En synk som hänger blockerar hela sparflödet. Trettio
            // sekunder räcker för en trög hemmaserver och är kort nog att
            // inte se ut som att appen frusit.
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| WebDavError::Unreachable(e.to_string()))
    }

    /// `SyncProvider` är synkron medan `reqwest` är asynkron.
    ///
    /// En egen runtime per anrop i stället för en delad: synken körs redan
    /// på en egen tråd (`spawn_background_sync_*`), sker sällan, och en
    /// global runtime hade betytt en till livstid att hålla reda på för
    /// två HTTP-anrop. Samma avvägning som `ssh::run_command` gör.
    fn block_on<F: std::future::Future>(future: F) -> Result<F::Output, WebDavError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| WebDavError::Unreachable(e.to_string()))?;
        Ok(runtime.block_on(future))
    }
}

impl SyncProvider for WebDavSyncProvider {
    fn pull(&self) -> std::io::Result<Option<SyncState>> {
        let client = Self::client()?;
        let (status, body) = Self::block_on(async {
            let response = client
                .get(&self.url)
                .basic_auth(&self.username, Some(&self.password))
                .send()
                .await
                .map_err(|e| WebDavError::Unreachable(e.to_string()))?;
            let status = response.status().as_u16();
            // Kroppen läses ALLTID, även vid 404 — annars måste svaret
            // hållas levande över matchningen nedan, och en 404-kropp är
            // ändå bara några byte felmeddelande.
            let body = response
                .text()
                .await
                .map_err(|e| WebDavError::Malformed(e.to_string()))?;
            Ok::<_, WebDavError>((status, body))
        })??;

        match classify(status)? {
            HttpOutcome::Missing => Ok(None),
            HttpOutcome::Ok => {
                // En tom fil är inte trasig — den kan ha skapats av en
                // avbruten push. Behandlas som "inget synkat än".
                if body.trim().is_empty() {
                    return Ok(None);
                }
                let state = serde_json::from_str(&body)
                    .map_err(|e| WebDavError::Malformed(e.to_string()))?;
                Ok(Some(state))
            }
        }
    }

    fn push(&self, state: &SyncState) -> std::io::Result<()> {
        let body = serde_json::to_string_pretty(state)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let client = Self::client()?;
        let status = Self::block_on(async {
            let response = client
                .put(&self.url)
                .basic_auth(&self.username, Some(&self.password))
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body)
                .send()
                .await
                .map_err(|e| WebDavError::Unreachable(e.to_string()))?;
            Ok::<_, WebDavError>(response.status().as_u16())
        })??;

        // PUT ska ge 201 (skapad) eller 204/200 (ersatt). En 404 här är
        // ETT FEL, till skillnad från vid pull: katalogen finns inte, och
        // att tyst svälja det hade betytt att synken såg ut att fungera
        // medan ingenting sparades.
        match classify(status)? {
            HttpOutcome::Ok => Ok(()),
            HttpOutcome::Missing => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "katalogen på servern finns inte — kontrollera URL:en",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hela poängen med `classify`: 404 är INTE ett fel. Ingen har synkat
    /// än, precis som en saknad fil i mapptransporten.
    #[test]
    fn a_missing_file_is_not_an_error_but_a_rejected_login_is() {
        assert_eq!(classify(200).unwrap(), HttpOutcome::Ok);
        assert_eq!(classify(201).unwrap(), HttpOutcome::Ok);
        assert_eq!(classify(204).unwrap(), HttpOutcome::Ok);
        assert_eq!(classify(404).unwrap(), HttpOutcome::Missing);
        assert_eq!(classify(410).unwrap(), HttpOutcome::Missing);

        assert!(matches!(classify(401), Err(WebDavError::Unauthorized)));
        assert!(matches!(classify(403), Err(WebDavError::Unauthorized)));
        assert!(matches!(classify(500), Err(WebDavError::Status(500))));
        assert!(matches!(classify(301), Err(WebDavError::Status(301))));
    }

    /// Felen ska gå att skilja åt i gränssnittet — ett nätverksfel och ett
    /// fel lösenord kräver olika saker av användaren.
    #[test]
    fn error_kinds_survive_the_conversion_to_io_error() {
        let unauthorized: std::io::Error = WebDavError::Unauthorized.into();
        assert_eq!(unauthorized.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(unauthorized.to_string().contains("användarnamn"));

        let unreachable: std::io::Error = WebDavError::Unreachable("timeout".into()).into();
        assert_eq!(unreachable.kind(), std::io::ErrorKind::ConnectionRefused);

        let malformed: std::io::Error = WebDavError::Malformed("bad json".into()).into();
        assert_eq!(malformed.kind(), std::io::ErrorKind::InvalidData);
    }

    /// End-to-end mot en RIKTIG HTTP-server som talar det WebDAV faktiskt
    /// behöver här: GET, PUT och basic auth.
    ///
    /// Servern är en trettio rader lång Python-stub i stället för en
    /// riktig Nextcloud — det som testas är providerns beteende mot
    /// statuskoder och kroppar, och det är identiskt oavsett vad som
    /// står i andra änden.
    #[test]
    #[ignore = "startar en lokal HTTP-server, kräver python3 — se ROADMAP.md"]
    fn full_round_trip_against_a_real_http_server() {
        let port = crate::test_support::reserve_port().expect("ingen ledig port");
        let dir = std::env::temp_dir().join(format!("bastion-webdav-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("server.py");
        std::fs::write(&script, PYTHON_STUB).unwrap();

        let mut child = std::process::Command::new("python3")
            .arg(&script)
            .arg(port.to_string())
            .arg(dir.join("store.json"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("kunde inte starta stubben");

        // Vänta tills den lyssnar.
        let mut ready = false;
        for _ in 0..50 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                ready = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(ready, "stubben började aldrig lyssna");

        let url = format!("http://127.0.0.1:{port}/bastion.json");
        let provider =
            WebDavSyncProvider::new(url.clone(), "anders".into(), "hemligt".into());

        // 1. Inget synkat än -> None, inte ett fel.
        assert!(provider.pull().expect("pull mot tom server").is_none());

        // 2. Push följt av pull ger tillbaka samma tillstånd.
        let mut state = SyncState::default();
        state.hosts.push(crate::host::Host::new(
            "webdav-test".into(),
            "10.0.0.1".into(),
            "anders".into(),
        ));
        provider.push(&state).expect("push misslyckades");

        let back = provider.pull().expect("pull efter push").expect("inget kom tillbaka");
        assert_eq!(back.hosts.len(), 1);
        assert_eq!(back.hosts[0].alias, "webdav-test");

        // 3. Fel lösenord ska ge Unauthorized och inget annat.
        let wrong = WebDavSyncProvider::new(url, "anders".into(), "fel".into());
        let err = wrong.pull().expect_err("fel lösenord skulle avvisats");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);

        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&dir);
    }

    const PYTHON_STUB: &str = r#"
import base64, json, os, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

PORT = int(sys.argv[1])
STORE = sys.argv[2]
EXPECTED = "Basic " + base64.b64encode(b"anders:hemligt").decode()

class Handler(BaseHTTPRequestHandler):
    def authed(self):
        if self.headers.get("Authorization") != EXPECTED:
            self.send_response(401)
            self.send_header("WWW-Authenticate", 'Basic realm="test"')
            self.end_headers()
            self.wfile.write(b"nope")
            return False
        return True

    def do_GET(self):
        if not self.authed():
            return
        if not os.path.exists(STORE):
            self.send_response(404)
            self.end_headers()
            self.wfile.write(b"not found")
            return
        body = open(STORE, "rb").read()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_PUT(self):
        if not self.authed():
            return
        length = int(self.headers.get("Content-Length", 0))
        open(STORE, "wb").write(self.rfile.read(length))
        self.send_response(201)
        self.end_headers()

    def log_message(self, *args):
        pass

HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
"#;
}
