//! OAuth2/PKCE-kontosynk mot Dropbox/Google Drive/OneDrive — port av
//! `Sources/SSHCore/OAuthPKCE.swift` + `OAuthToken.swift` +
//! `App/OAuthProviders.swift`, samt (delvis) den RIKTIGA HTTP-logiken i
//! `App/OAuthAccountManager.swift`/`OAuthTokenStore.swift`.
//!
//! **Avgränsning (medvetet, se ROADMAP.md "Kvar"):** det här är KÄRNAN —
//! PKCE-kryptot, token-strukturerna, provider­konfigurationen, och det
//! RIKTIGA token-utbytet/token-förnyelsen över HTTP (`reqwest`, testat mot
//! en genuin lokal HTTP-server, samma teknik som `s3.rs`). Den INTERAKTIVA
//! inloggningen — öppna en webbläsare mot `authorization_endpoint`, fånga
//! redirect-URI:n servern skickar tillbaka koden på — är INTE med än. På
//! Apple-plattformar löser `ASWebAuthenticationSession` det åt Swift-sidan;
//! Linux har ingen motsvarighet inbyggd i GTK, och vilken strategi som
//! passar (en lokal loopback-HTTP-lyssnare enligt RFC 8252, kontra att
//! registrera `se.denied.bastion://`-schemat via `xdg-mime`/en `.desktop`-
//! fil) är ett öppet designval som inte avgörs här.
//!
//! **Lagring:** en enkel lokal JSON-fil (`~/.bastion/oauth_tokens.json`) —
//! samma skyddsnivå som S3-kopplingarnas `secret_access_key`/WireGuard-
//! privatnycklar redan har i den här klienten (`s3.rs`/`wireguard.rs`),
//! INTE macOS Keychain (som Swift-sidan använder — Linux har ingen
//! universell motsvarighet över skrivbordsmiljöer; freedesktop Secret
//! Service är en egen beroende-yta som inte är motiverad förrän
//! inloggningsflödet ovan faktiskt finns).
//!
//! `#![allow(dead_code)]`: hela modulen är medvetet INTE kopplad in i
//! `main.rs` än — den väntar på att den interaktiva inloggnings-UI:n
//! (nästa PR, se ovan) faktiskt anropar den. Bara testad, inte använd,
//! samma motivering som `WireGuardProfileStore::get` i `wireguard.rs`.
#![allow(dead_code)]

use base64::Engine;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// PKCE (RFC 7636) code verifier: 32 råa slumpade byte, base64url utan
/// padding (43 tecken) — inom RFC:ns tillåtna 43–128.
pub fn pkce_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// `code_challenge` för `code_challenge_method=S256`: SHA256 av verifiern,
/// base64url utan padding.
pub fn pkce_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
}

/// Rått svar från en token-endpoint (`authorization_code`- eller
/// `refresh_token`-grant).
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthTokenResponse {
    #[serde(rename = "access_token")]
    pub access_token: String,
    #[serde(rename = "refresh_token")]
    pub refresh_token: Option<String>,
    #[serde(rename = "expires_in")]
    pub expires_in: Option<f64>,
}

/// Det som faktiskt sparas lokalt — absolut utgångstid (Unix-sekunder,
/// INTE `host.rs`s `ReferenceDate`-epok: den här filen delas aldrig med
/// Swift-sidan eller synkas mellan enheter, så det finns ingen anledning
/// att matcha dess epokval) i stället för leverantörens relativa
/// `expires_in`, så vi slipper räkna om vid varje läsning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredOAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<f64>,
}

impl StoredOAuthToken {
    /// `previous_refresh_token`: de flesta `refresh_token`-svar innehåller
    /// ingen ny `refresh_token` — den gamla måste bevaras, annars tappar vi
    /// förmågan att förnya igen.
    pub fn from_response(response: &OAuthTokenResponse, previous_refresh_token: Option<&str>, now: f64) -> Self {
        StoredOAuthToken {
            access_token: response.access_token.clone(),
            refresh_token: response
                .refresh_token
                .clone()
                .or_else(|| previous_refresh_token.map(String::from)),
            // 60 s marginal så vi förnyar strax innan utgång, inte precis vid den.
            expires_at: response.expires_in.map(|secs| now + secs - 60.0),
        }
    }

    /// `nil` utgångstid (leverantören svarade utan `expires_in`) tolkas som
    /// "fortfarande giltig" — ett 401 vid faktisk användning får trigga förnyelse.
    pub fn is_expired(&self, now: f64) -> bool {
        match self.expires_at {
            Some(exp) => now >= exp,
            None => false,
        }
    }
}

pub fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("systemklockan är före 1970")
        .as_secs_f64()
}

/// Beskriver en OAuth2-leverantör för PKCE-inloggning mot en app-scopad
/// mapp (Dropbox "App folder"-behörighet, Google Drives
/// `drive.appdata`-scope, OneDrives `Files.ReadWrite.AppFolder`) — appen
/// ber aldrig om åtkomst till hela kontot. Samma tre leverantörer/samma
/// registrerade värden som `App/OAuthProviders.swift` (`clientID` tom tills
/// leverantören är registrerad — se dess kommentar för exakta steg).
#[derive(Debug, Clone)]
pub struct OAuthProviderConfig {
    pub id: &'static str,
    pub display_name: &'static str,
    pub authorization_endpoint: &'static str,
    pub token_endpoint: &'static str,
    pub scope: &'static str,
    pub redirect_uri: &'static str,
    pub client_id: &'static str,
}

impl OAuthProviderConfig {
    pub fn is_configured(&self) -> bool {
        !self.client_id.is_empty()
    }
}

pub fn dropbox() -> OAuthProviderConfig {
    OAuthProviderConfig {
        id: "dropbox",
        display_name: "Dropbox",
        authorization_endpoint: "https://www.dropbox.com/oauth2/authorize",
        token_endpoint: "https://api.dropboxapi.com/oauth2/token",
        scope: "files.content.write files.content.read",
        redirect_uri: "se.denied.bastion://oauth/dropbox",
        client_id: "ira5qtb04w4qikk",
    }
}

pub fn google_drive() -> OAuthProviderConfig {
    OAuthProviderConfig {
        id: "googledrive",
        display_name: "Google Drive",
        authorization_endpoint: "https://accounts.google.com/o/oauth2/v2/auth",
        token_endpoint: "https://oauth2.googleapis.com/token",
        scope: "https://www.googleapis.com/auth/drive.appdata",
        redirect_uri: "se.denied.bastion://oauth/googledrive",
        client_id: "",
    }
}

pub fn one_drive() -> OAuthProviderConfig {
    OAuthProviderConfig {
        id: "onedrive",
        display_name: "OneDrive",
        authorization_endpoint: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        token_endpoint: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        scope: "Files.ReadWrite.AppFolder offline_access",
        redirect_uri: "se.denied.bastion://oauth/onedrive",
        client_id: "",
    }
}

pub fn all_providers() -> Vec<OAuthProviderConfig> {
    vec![dropbox(), google_drive(), one_drive()]
}

/// Bygger auktoriseringsURL:en användaren ska öppnas mot — ren funktion,
/// inget nätverk. Motsvarar `URLComponents`-uppbyggnaden i
/// `OAuthAccountManager.login`.
pub fn build_authorize_url(provider: &OAuthProviderConfig, challenge: &str, state: &str) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(provider.authorization_endpoint).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("client_id", provider.client_id)
        .append_pair("redirect_uri", provider.redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", provider.scope)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);
    Ok(url)
}

/// Byter en auktoriseringskod mot ett åtkomsttoken-par. Motsvarar
/// `OAuthAccountManager.exchangeCodeForToken` — RIKTIGT HTTP-anrop
/// (`reqwest`), inget mockat lager.
pub async fn exchange_code_for_token(
    client: &reqwest::Client,
    provider: &OAuthProviderConfig,
    code: &str,
    verifier: &str,
) -> Result<StoredOAuthToken, String> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", provider.redirect_uri),
        ("client_id", provider.client_id),
        ("code_verifier", verifier),
    ];
    let response = request_token(client, provider.token_endpoint, &form).await?;
    Ok(StoredOAuthToken::from_response(&response, None, unix_now()))
}

/// Förnyar ett utgånget åtkomsttoken via en sparad `refresh_token`.
/// Motsvarar `OAuthTokenStore.refresh`.
pub async fn refresh_access_token(
    client: &reqwest::Client,
    provider: &OAuthProviderConfig,
    refresh_token: &str,
) -> Result<StoredOAuthToken, String> {
    let form = [("grant_type", "refresh_token"), ("refresh_token", refresh_token), ("client_id", provider.client_id)];
    let response = request_token(client, provider.token_endpoint, &form).await?;
    Ok(StoredOAuthToken::from_response(&response, Some(refresh_token), unix_now()))
}

async fn request_token(client: &reqwest::Client, token_endpoint: &str, form: &[(&str, &str)]) -> Result<OAuthTokenResponse, String> {
    let resp = client
        .post(token_endpoint)
        .form(form)
        .send()
        .await
        .map_err(|e| format!("token-begäran misslyckades: {e}"))?;
    let status = resp.status();
    let bytes = resp.bytes().await.map_err(|e| format!("kunde inte läsa svaret: {e}"))?;
    if !status.is_success() {
        return Err(format!("token-endpoint svarade {status}: {}", String::from_utf8_lossy(&bytes)));
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("kunde inte tolka svaret: {e} ({})", String::from_utf8_lossy(&bytes)))
}

/// Läser/skriver token lokalt (se modulkommentaren om avgränsningen mot
/// en riktig OS-keychain) och förnyar tyst via `refresh_token`.
pub struct OAuthTokenStore {
    path: std::path::PathBuf,
}

impl OAuthTokenStore {
    pub fn default_path() -> std::path::PathBuf {
        dirs::home_dir().expect("kunde inte hitta hemkatalogen").join(".bastion/oauth_tokens.json")
    }

    pub fn open(path: std::path::PathBuf) -> Self {
        OAuthTokenStore { path }
    }

    fn load_all(&self) -> std::collections::HashMap<String, StoredOAuthToken> {
        let Ok(data) = std::fs::read_to_string(&self.path) else {
            return std::collections::HashMap::new();
        };
        serde_json::from_str(&data).unwrap_or_default()
    }

    fn save_all(&self, all: &std::collections::HashMap<String, StoredOAuthToken>) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, serde_json::to_string(all).expect("StoredOAuthToken serialiserar alltid"))
    }

    pub fn is_logged_in(&self, provider: &OAuthProviderConfig) -> bool {
        self.load_all().contains_key(provider.id)
    }

    pub fn logout(&self, provider: &OAuthProviderConfig) -> std::io::Result<()> {
        let mut all = self.load_all();
        all.remove(provider.id);
        self.save_all(&all)
    }

    pub fn load(&self, provider: &OAuthProviderConfig) -> Option<StoredOAuthToken> {
        self.load_all().get(provider.id).cloned()
    }

    pub fn save(&self, provider: &OAuthProviderConfig, token: &StoredOAuthToken) -> std::io::Result<()> {
        let mut all = self.load_all();
        all.insert(provider.id.to_string(), token.clone());
        self.save_all(&all)
    }

    /// Hämtar ett giltigt åtkomsttoken, förnyar tyst via `refresh_token` om
    /// det gått ut. Motsvarar `OAuthTokenStore.validAccessToken`.
    pub async fn valid_access_token(&self, client: &reqwest::Client, provider: &OAuthProviderConfig) -> Result<String, String> {
        let mut token = self.load(provider).ok_or("inte inloggad")?;
        if token.is_expired(unix_now()) {
            let Some(refresh_token) = token.refresh_token.clone() else {
                return Err("token har gått ut och saknar refresh_token".into());
            };
            token = refresh_access_token(client, provider, &refresh_token).await?;
            self.save(provider, &token).map_err(|e| format!("kunde inte spara det förnyade tokenet: {e}"))?;
        }
        Ok(token.access_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // MARK: - PKCE (RFC 7636)

    #[test]
    fn challenge_matches_the_rfc_7636_vector() {
        // Känd testvektor ur RFC 7636 §4.1/§4.2 (S256).
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(pkce_challenge(verifier), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn verifier_is_url_safe_and_the_right_length() {
        for _ in 0..20 {
            let v = pkce_verifier();
            assert_eq!(v.len(), 43); // 32 råa byte -> 43 base64url-tecken utan padding
            assert!(v.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'));
            assert!(!v.contains('+') && !v.contains('/') && !v.contains('='));
        }
    }

    #[test]
    fn verifiers_are_not_reused() {
        assert_ne!(pkce_verifier(), pkce_verifier());
    }

    #[test]
    fn challenge_is_deterministic_for_the_same_verifier() {
        let v = pkce_verifier();
        assert_eq!(pkce_challenge(&v), pkce_challenge(&v));
    }

    // MARK: - Token-utgång

    fn response(access: &str, refresh: Option<&str>, expires_in: Option<f64>) -> OAuthTokenResponse {
        OAuthTokenResponse {
            access_token: access.to_string(),
            refresh_token: refresh.map(String::from),
            expires_in,
        }
    }

    #[test]
    fn not_expired_just_after_issue() {
        let now = unix_now();
        let token = StoredOAuthToken::from_response(&response("a", Some("r"), Some(3600.0)), None, now);
        assert!(!token.is_expired(now));
    }

    #[test]
    fn expired_past_expiry_minus_margin() {
        let issued_at = 0.0;
        let token = StoredOAuthToken::from_response(&response("a", Some("r"), Some(3600.0)), None, issued_at);
        // expires_at = issued_at + 3600 - 60 = issued_at + 3540
        assert!(!token.is_expired(issued_at + 3539.0));
        assert!(token.is_expired(issued_at + 3541.0));
    }

    #[test]
    fn no_expires_in_never_expires() {
        let token = StoredOAuthToken::from_response(&response("a", Some("r"), None), None, unix_now());
        assert_eq!(token.expires_at, None);
        assert!(!token.is_expired(unix_now()));
    }

    #[test]
    fn refresh_token_carries_over_when_the_provider_omits_it() {
        // De flesta refresh_token-svar innehåller ingen ny refresh_token —
        // den gamla måste bevaras, annars tappar vi förmågan att förnya igen.
        let token = StoredOAuthToken::from_response(&response("new-access", None, Some(3600.0)), Some("old-refresh"), unix_now());
        assert_eq!(token.refresh_token.as_deref(), Some("old-refresh"));
        assert_eq!(token.access_token, "new-access");
    }

    #[test]
    fn refresh_token_replaced_when_the_provider_rotates_it() {
        let token = StoredOAuthToken::from_response(
            &response("new-access", Some("rotated-refresh"), Some(3600.0)),
            Some("old-refresh"),
            unix_now(),
        );
        assert_eq!(token.refresh_token.as_deref(), Some("rotated-refresh"));
    }

    #[test]
    fn stored_token_json_round_trips() {
        // Ett rundtal `now` (inte `unix_now()`s bråkdelssekunder) — annars
        // introducerar `f64`-serialiseringens flyttalsprecision en
        // spurios olikhet i sista decimalen, orelaterat till det
        // faktiska round-trip-beteendet testet vill bevisa.
        let token = StoredOAuthToken::from_response(&response("a", Some("r"), Some(3600.0)), None, 1_700_000_000.0);
        let json = serde_json::to_string(&token).unwrap();
        let decoded: StoredOAuthToken = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, token);
    }

    // MARK: - Provider­konfiguration

    #[test]
    fn dropbox_is_configured_google_and_onedrive_are_not_yet() {
        assert!(dropbox().is_configured());
        assert!(!google_drive().is_configured());
        assert!(!one_drive().is_configured());
    }

    #[test]
    fn authorize_url_contains_every_pkce_parameter() {
        let provider = dropbox();
        let url = build_authorize_url(&provider, "the-challenge", "the-state").unwrap();
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs.get("client_id").unwrap(), provider.client_id);
        assert_eq!(pairs.get("redirect_uri").unwrap(), provider.redirect_uri);
        assert_eq!(pairs.get("response_type").unwrap(), "code");
        assert_eq!(pairs.get("scope").unwrap(), provider.scope);
        assert_eq!(pairs.get("code_challenge").unwrap(), "the-challenge");
        assert_eq!(pairs.get("code_challenge_method").unwrap(), "S256");
        assert_eq!(pairs.get("state").unwrap(), "the-state");
    }

    // MARK: - Genuint token-utbyte/förnyelse mot en riktig, minimal HTTP-server

    /// Samma teknik som `s3.rs`s `spawn_fake_s3_server`: en rå TCP/HTTP-1.1-
    /// server som spelar in begäran och svarar med ett fördefinierat JSON-
    /// svar. Bevisar att HELA vägen — formulärkodning, riktiga HTTP-anrop
    /// via `reqwest`, JSON-tolkning av svaret — fungerar ihop.
    async fn spawn_fake_token_server(status_line: &'static str, response_body: &'static str) -> (u16, tokio::sync::oneshot::Receiver<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = socket.read(&mut buf).await.unwrap_or(0);
            let request_head = String::from_utf8_lossy(&buf[..n]).into_owned();
            let body = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = socket.write_all(body.as_bytes()).await;
            let _ = socket.shutdown().await;
            let _ = tx.send(request_head);
        });
        (port, rx)
    }

    fn test_provider(token_endpoint: &'static str) -> OAuthProviderConfig {
        OAuthProviderConfig {
            id: "test",
            display_name: "Test",
            authorization_endpoint: "https://example.invalid/authorize",
            token_endpoint,
            scope: "scope",
            redirect_uri: "http://127.0.0.1/callback",
            client_id: "test-client-id",
        }
    }

    #[tokio::test]
    async fn exchange_code_for_token_sends_the_right_form_fields_and_parses_the_response() {
        let (port, rx) = spawn_fake_token_server(
            "HTTP/1.1 200 OK",
            r#"{"access_token":"AT","refresh_token":"RT","expires_in":3600}"#,
        )
        .await;
        let endpoint: &'static str = Box::leak(format!("http://127.0.0.1:{port}/token").into_boxed_str());
        let provider = test_provider(endpoint);
        let client = reqwest::Client::new();

        let token = exchange_code_for_token(&client, &provider, "the-code", "the-verifier")
            .await
            .expect("token-utbytet misslyckades");
        assert_eq!(token.access_token, "AT");
        assert_eq!(token.refresh_token.as_deref(), Some("RT"));
        assert!(token.expires_at.is_some());

        let request_head = rx.await.expect("servern fick aldrig någon begäran");
        assert!(request_head.starts_with("POST /token HTTP/1.1"), "fel begäranrad: {request_head}");
        assert!(
            request_head.contains("content-type: application/x-www-form-urlencoded"),
            "saknar rätt Content-Type: {request_head}"
        );
        assert!(request_head.contains("grant_type=authorization_code"), "{request_head}");
        assert!(request_head.contains("code=the-code"), "{request_head}");
        assert!(request_head.contains("code_verifier=the-verifier"), "{request_head}");
        assert!(request_head.contains(&format!("client_id={}", provider.client_id)), "{request_head}");
    }

    #[tokio::test]
    async fn refresh_access_token_carries_over_the_refresh_token_when_omitted_and_sends_the_right_grant_type() {
        let (port, rx) = spawn_fake_token_server("HTTP/1.1 200 OK", r#"{"access_token":"NEW-AT","expires_in":3600}"#).await;
        let endpoint: &'static str = Box::leak(format!("http://127.0.0.1:{port}/token").into_boxed_str());
        let provider = test_provider(endpoint);
        let client = reqwest::Client::new();

        let token = refresh_access_token(&client, &provider, "old-refresh").await.expect("förnyelsen misslyckades");
        assert_eq!(token.access_token, "NEW-AT");
        assert_eq!(token.refresh_token.as_deref(), Some("old-refresh"));

        let request_head = rx.await.expect("servern fick aldrig någon begäran");
        assert!(request_head.contains("grant_type=refresh_token"), "{request_head}");
        assert!(request_head.contains("refresh_token=old-refresh"), "{request_head}");
    }

    #[tokio::test]
    async fn a_non_2xx_response_becomes_a_clear_error_not_a_silent_success() {
        let (port, _rx) = spawn_fake_token_server("HTTP/1.1 400 Bad Request", r#"{"error":"invalid_grant"}"#).await;
        let endpoint: &'static str = Box::leak(format!("http://127.0.0.1:{port}/token").into_boxed_str());
        let provider = test_provider(endpoint);
        let client = reqwest::Client::new();

        let err = exchange_code_for_token(&client, &provider, "bad-code", "v")
            .await
            .expect_err("ett 400-svar ska ge Err, inte Ok");
        assert!(err.contains("invalid_grant"), "felet ska innehålla serversvaret, fick: {err}");
    }

    // MARK: - Lokal lagring

    fn test_store() -> (OAuthTokenStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("bastion-oauth-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("oauth_tokens.json");
        (OAuthTokenStore::open(path.clone()), dir)
    }

    #[test]
    fn store_round_trips_save_load_logout_per_provider() {
        let (store, dir) = test_store();
        let dropbox = dropbox();
        let google = google_drive();
        assert!(!store.is_logged_in(&dropbox));
        assert_eq!(store.load(&dropbox), None);

        let token = StoredOAuthToken { access_token: "AT".into(), refresh_token: Some("RT".into()), expires_at: Some(123.0) };
        store.save(&dropbox, &token).unwrap();
        assert!(store.is_logged_in(&dropbox));
        assert_eq!(store.load(&dropbox), Some(token));
        // En annan leverantör ska inte påverkas av dropbox-posten.
        assert!(!store.is_logged_in(&google));

        store.logout(&dropbox).unwrap();
        assert!(!store.is_logged_in(&dropbox));
        assert_eq!(store.load(&dropbox), None);

        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn valid_access_token_refreshes_silently_when_expired() {
        let (port, rx) = spawn_fake_token_server("HTTP/1.1 200 OK", r#"{"access_token":"REFRESHED-AT","expires_in":3600}"#).await;
        let endpoint: &'static str = Box::leak(format!("http://127.0.0.1:{port}/token").into_boxed_str());
        let provider = test_provider(endpoint);
        let (store, dir) = test_store();
        let expired = StoredOAuthToken { access_token: "OLD-AT".into(), refresh_token: Some("RT".into()), expires_at: Some(0.0) };
        store.save(&provider, &expired).unwrap();

        let client = reqwest::Client::new();
        let access_token = store.valid_access_token(&client, &provider).await.expect("förnyelsen misslyckades");
        assert_eq!(access_token, "REFRESHED-AT");
        // Den förnyade token:en ska ha sparats — en efterföljande läsning
        // ska ge den nya, inte den gamla.
        assert_eq!(store.load(&provider).unwrap().access_token, "REFRESHED-AT");

        let request_head = rx.await.expect("servern fick aldrig någon begäran");
        assert!(request_head.contains("grant_type=refresh_token"), "{request_head}");

        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn valid_access_token_reuses_a_still_valid_token_without_any_http_call() {
        let (store, dir) = test_store();
        let provider = dropbox();
        let fresh = StoredOAuthToken::from_response(&response("AT", Some("RT"), Some(3600.0)), None, unix_now());
        store.save(&provider, &fresh).unwrap();

        // Ingen server startas alls — ett HTTP-anrop hade panikat/timeoutat
        // mot en icke-lyssnande port, vilket beviset kräver.
        let client = reqwest::Client::new();
        let access_token = store.valid_access_token(&client, &provider).await.expect("skulle inte behöva förnyas");
        assert_eq!(access_token, fresh.access_token);

        std::fs::remove_dir_all(dir).ok();
    }
}
