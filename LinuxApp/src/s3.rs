//! S3-kompatibel objektlagringsklient (AWS Signature Version 4) — fungerar
//! mot riktig AWS S3 OCH mot S3-kompatibla leverantörer (Ceph RGW m.fl.),
//! eftersom SigV4 är en delad, väldokumenterad spec, inte en AWS-specifik
//! hemlighet. Port av `Sources/SSHCore/S3Client.swift` +
//! `S3ConnectionStore.swift`. Motiveras av VISION.md "Native
//! filhanterare-integration och molnlagring som filkälla": AWS/S3-
//! kompatibel lagring har inget konsument-OAuth, användaren klistrar in
//! sina egna nycklar.
//!
//! Signeringen (`sign`) är verifierad mot EXAKT samma fixerade,
//! icke-tidsberoende testvektor som Swift-sidans `S3ClientTests.swift`
//! (`testSigV4MatchesVerifiedReferenceVector`) — härledd ur en oberoende
//! Python-referensimplementation som fick ett genuint 200 OK mot en RIKTIG
//! S3-kompatibel tjänst (Hostups `s3.hostup.se`, Ceph RGW). Om denna port
//! ger samma `Authorization`-header för samma indata är algoritmen bevisat
//! korrekt, inte bara "ser rimlig ut".
//!
//! Path-style URL:er (`https://endpoint/bucket/key`), inte virtual-hosted
//! (`https://bucket.endpoint/key`) — samma val som Swift-sidan (Ceph RGW
//! och de flesta S3-kompatibla leverantörer stödjer path-style universellt,
//! virtual-hosted kräver wildcard-DNS som inte alla leverantörer sätter upp).

use crate::host::ReferenceDate;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Bucket {
    pub name: String,
    /// Rå ISO8601-text, inte tolkad till ett datumvärde — appen visar den
    /// bara, jämför den aldrig (samma information Swift-sidans `Date?` bär,
    /// bara utan en extra parsningsomgång som inget kallar).
    pub creation_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Object {
    pub key: String,
    pub size: i64,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum S3Error {
    Transport(String),
    HttpError { status: u16, code: String, message: String },
    MalformedResponse,
}

impl std::fmt::Display for S3Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            S3Error::Transport(e) => write!(f, "{e}"),
            S3Error::HttpError { status, code, message } => {
                write!(f, "{status} {code}: {message}")
            }
            S3Error::MalformedResponse => write!(f, "ogiltigt svar från servern"),
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex_encode(&hasher.finalize())
}

fn hmac_bytes(key: &[u8], message: &str) -> Vec<u8> {
    let mut mac =
        HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepterar nycklar av alla längder");
    mac.update(message.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Dagens UTC-tidsstämpel i SigV4:s `amz-date`-format (`yyyyMMdd'T'HHmmss'Z'`).
pub fn iso_date_now() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

pub struct SignedRequest {
    pub authorization_header: String,
    pub amz_date: String,
    pub content_sha256: String,
}

/// Bygger canonical request + string-to-sign + signatur och returnerar en
/// klar `Authorization`-header. `path` måste redan vara URI-encodad (inte
/// innehålla frågesträngen); `query_string` är den redan sorterade,
/// encodade canonical query-strängen (tom sträng om ingen).
#[allow(clippy::too_many_arguments)]
pub fn sign(
    method: &str,
    host: &str,
    path: &str,
    query_string: &str,
    payload: &[u8],
    region: &str,
    credentials: &S3Credentials,
    amz_date: &str,
) -> SignedRequest {
    let datestamp = &amz_date[..8];
    let content_hash = sha256_hex(payload);

    let canonical_headers =
        format!("host:{host}\nx-amz-content-sha256:{content_hash}\nx-amz-date:{amz_date}\n");
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";

    let canonical_request = [
        method,
        path,
        query_string,
        &canonical_headers,
        signed_headers,
        &content_hash,
    ]
    .join("\n");

    let credential_scope = format!("{datestamp}/{region}/s3/aws4_request");
    let string_to_sign = [
        "AWS4-HMAC-SHA256",
        amz_date,
        &credential_scope,
        &sha256_hex(canonical_request.as_bytes()),
    ]
    .join("\n");

    let k_date = hmac_bytes(
        format!("AWS4{}", credentials.secret_access_key).as_bytes(),
        datestamp,
    );
    let k_region = hmac_bytes(&k_date, region);
    let k_service = hmac_bytes(&k_region, "s3");
    let k_signing = hmac_bytes(&k_service, "aws4_request");
    let signature = hex_encode(&hmac_bytes(&k_signing, &string_to_sign));

    let auth = format!(
        "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
        credentials.access_key_id
    );

    SignedRequest {
        authorization_header: auth,
        amz_date: amz_date.to_string(),
        content_sha256: content_hash,
    }
}

/// URI-encoding enligt SigV4-reglerna (RFC 3986). Kodar ETT segment
/// (inklusive `/`, till skillnad från hela sökvägen) — hela sökvägen sätts
/// ihop med `/` mellan de encodade segmenten, se `encode_path`.
fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        let c = byte as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~') {
            out.push(c);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out
}

fn encode_path(segments: &[String]) -> String {
    format!(
        "/{}",
        segments
            .iter()
            .map(|s| encode_path_segment(s))
            .collect::<Vec<_>>()
            .join("/")
    )
}

pub struct S3Client {
    endpoint_scheme: String,
    endpoint_host: String,
    endpoint_port: Option<u16>,
    region: String,
    credentials: S3Credentials,
    http: reqwest::Client,
}

impl S3Client {
    pub fn new(endpoint: &str, region: String, credentials: S3Credentials) -> Result<Self, S3Error> {
        let url = reqwest::Url::parse(endpoint).map_err(|_| S3Error::MalformedResponse)?;
        let host = url.host_str().ok_or(S3Error::MalformedResponse)?.to_string();
        Ok(S3Client {
            endpoint_scheme: url.scheme().to_string(),
            endpoint_host: host,
            endpoint_port: url.port(),
            region,
            credentials,
            http: reqwest::Client::new(),
        })
    }

    /// `Host`-värdet som faktiskt måste signeras — en icke-standardport
    /// (t.ex. en lokal MinIO-instans på :9000) måste synas i den signerade
    /// headern exakt som den skickas, annars underkänns signaturen av
    /// servern (samma CodeRabbit-fynd som Swift-sidan redan fångat, PR #90).
    fn host(&self) -> String {
        match self.endpoint_port {
            None => self.endpoint_host.clone(),
            Some(port) => {
                let default_port = if self.endpoint_scheme == "https" { 443 } else { 80 };
                if port == default_port {
                    self.endpoint_host.clone()
                } else {
                    format!("{}:{port}", self.endpoint_host)
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn request(
        &self,
        method: reqwest::Method,
        path_segments: &[String],
        query_items: &[(String, String)],
        body: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<(Vec<u8>, u16), S3Error> {
        let path = if path_segments.is_empty() {
            "/".to_string()
        } else {
            encode_path(path_segments)
        };
        let mut sorted_query = query_items.to_vec();
        sorted_query.sort_by(|a, b| a.0.cmp(&b.0));
        let query_string = sorted_query
            .iter()
            .map(|(k, v)| format!("{k}={}", encode_path_segment(v)))
            .collect::<Vec<_>>()
            .join("&");

        let host = self.host();
        let amz_date = iso_date_now();
        let signed = sign(
            method.as_str(),
            &host,
            &path,
            &query_string,
            &body,
            &self.region,
            &self.credentials,
            &amz_date,
        );

        // Bygger URL:en som en färdig sträng (inte via `Url`s egna
        // `set_path`/`set_query`-mutatorer) — `path`/`query_string` är
        // REDAN percent-encodade enligt SigV4:s exakta regler, och `url`-
        // kratets mutatorer skulle percent-encoda dem EN GÅNG TILL (t.ex.
        // "%20" -> "%2520"), vilket bryter signaturen. `Url::parse` på en
        // redan korrekt formad URI-sträng encodar INTE om giltiga
        // procent-tripplar — samma anledning som Swift-sidan sätter
        // `URLComponents.percentEncodedPath` direkt istället för `.path`.
        let query_part = if query_string.is_empty() {
            String::new()
        } else {
            format!("?{query_string}")
        };
        let url_string = format!("{}://{host}{path}{query_part}", self.endpoint_scheme);

        let mut req = self
            .http
            .request(method, &url_string)
            .header("x-amz-date", &signed.amz_date)
            .header("x-amz-content-sha256", &signed.content_sha256)
            .header("Authorization", &signed.authorization_header);
        if let Some(ct) = content_type {
            req = req.header("Content-Type", ct);
        }
        if !body.is_empty() {
            req = req.body(body);
        }

        let response = req
            .send()
            .await
            .map_err(|e| S3Error::Transport(e.to_string()))?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| S3Error::Transport(e.to_string()))?;
        Ok((bytes.to_vec(), status))
    }

    /// Som `request`, men lämnar tillbaka svaret OLÄST — anroparen får
    /// själv strömma kroppen. Delar all signerings-/URL-logik med
    /// `request` via `signed_request` nedan; enda skillnaden är att den
    /// här INTE anropar `.bytes()`.
    async fn send_streaming(
        &self,
        method: reqwest::Method,
        path_segments: &[String],
    ) -> Result<reqwest::Response, S3Error> {
        let path = encode_path(path_segments);
        let host = self.host();
        let amz_date = iso_date_now();
        let signed = sign(
            method.as_str(),
            &host,
            &path,
            "",
            &[],
            &self.region,
            &self.credentials,
            &amz_date,
        );
        // Samma "bygg URL:en som färdig sträng"-resonemang som i
        // `request` — se kommentaren där.
        let url_string = format!("{}://{host}{path}", self.endpoint_scheme);
        self.http
            .request(method, &url_string)
            .header("x-amz-date", &signed.amz_date)
            .header("x-amz-content-sha256", &signed.content_sha256)
            .header("Authorization", &signed.authorization_header)
            .send()
            .await
            .map_err(|e| S3Error::Transport(e.to_string()))
    }

    fn require_success(data: &[u8], status: u16) -> Result<(), S3Error> {
        if (200..300).contains(&status) {
            return Ok(());
        }
        let (code, message) = parse_error(data);
        Err(S3Error::HttpError { status, code, message })
    }

    pub async fn list_buckets(&self) -> Result<Vec<S3Bucket>, S3Error> {
        let (data, status) = self
            .request(reqwest::Method::GET, &[], &[], Vec::new(), None)
            .await?;
        Self::require_success(&data, status)?;
        Ok(parse_buckets(&data))
    }

    pub async fn create_bucket(&self, name: &str) -> Result<(), S3Error> {
        let (data, status) = self
            .request(reqwest::Method::PUT, &[name.to_string()], &[], Vec::new(), None)
            .await?;
        Self::require_success(&data, status)
    }

    pub async fn delete_bucket(&self, name: &str) -> Result<(), S3Error> {
        let (data, status) = self
            .request(reqwest::Method::DELETE, &[name.to_string()], &[], Vec::new(), None)
            .await?;
        Self::require_success(&data, status)
    }

    pub async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<S3Object>, S3Error> {
        let mut query = vec![("list-type".to_string(), "2".to_string())];
        if let Some(p) = prefix {
            query.push(("prefix".to_string(), p.to_string()));
        }
        let (data, status) = self
            .request(reqwest::Method::GET, &[bucket.to_string()], &query, Vec::new(), None)
            .await?;
        Self::require_success(&data, status)?;
        Ok(parse_objects(&data))
    }

    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<(), S3Error> {
        let (response_data, status) = self
            .request(
                reqwest::Method::PUT,
                &[bucket.to_string(), key.to_string()],
                &[],
                data,
                content_type,
            )
            .await?;
        Self::require_success(&response_data, status)
    }

    /// Hämtar hela objektet till MINNET. Bara för små, kända objekt —
    /// använd `get_object_to_file` för allt användaren laddar ner, se
    /// dess dokumentation.
    #[allow(dead_code)]
    pub async fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>, S3Error> {
        let (data, status) = self
            .request(
                reqwest::Method::GET,
                &[bucket.to_string(), key.to_string()],
                &[],
                Vec::new(),
                None,
            )
            .await?;
        Self::require_success(&data, status)?;
        Ok(data)
    }

    /// Strömmar objektet direkt till `destination`, bit för bit, utan att
    /// någon gång hålla hela filen i minnet.
    ///
    /// `get_object` (ovan) läser hela kroppen via `Response::bytes()` —
    /// för en S3-bucket, där flergigabyte-objekt (diskavbildningar,
    /// backuper, videofiler) är helt vardagliga, hade en nedladdning
    /// därmed krävt lika mycket RAM som filen är stor och i praktiken
    /// dödat appen. Övriga anrop (`list_buckets`/`list_objects`/fel-
    /// svar) är små XML-dokument och läses fortfarande i ett svep.
    ///
    /// Skriver till en temporär fil i SAMMA katalog och byter namn först
    /// när hela hämtningen lyckats — en avbruten nedladdning lämnar
    /// aldrig en halv fil på målsökvägen (samma resonemang som
    /// `external_binary_fetcher::fetch`).
    pub async fn get_object_to_file(&self, bucket: &str, key: &str, destination: &std::path::Path) -> Result<(), S3Error> {
        use tokio::io::AsyncWriteExt;

        let response = self
            .send_streaming(reqwest::Method::GET, &[bucket.to_string(), key.to_string()])
            .await?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            // Felsvar är små XML-dokument — läs dem i ett svep för att
            // kunna ge samma tydliga fel som övriga anrop.
            let data = response.bytes().await.map_err(|e| S3Error::Transport(e.to_string()))?;
            return Err(Self::require_success(&data, status).unwrap_err());
        }

        let parent = destination.parent().unwrap_or_else(|| std::path::Path::new("."));
        let tmp = parent.join(format!(
            ".{}.{}.part",
            destination.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "download".to_string()),
            uuid::Uuid::new_v4()
        ));

        let result = async {
            let mut file = tokio::fs::File::create(&tmp).await.map_err(|e| S3Error::Transport(e.to_string()))?;
            let mut response = response;
            while let Some(chunk) = response.chunk().await.map_err(|e| S3Error::Transport(e.to_string()))? {
                file.write_all(&chunk).await.map_err(|e| S3Error::Transport(e.to_string()))?;
            }
            file.flush().await.map_err(|e| S3Error::Transport(e.to_string()))?;
            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                tokio::fs::rename(&tmp, destination).await.map_err(|e| S3Error::Transport(e.to_string()))?;
                Ok(())
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp).await;
                Err(e)
            }
        }
    }

    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), S3Error> {
        let (data, status) = self
            .request(
                reqwest::Method::DELETE,
                &[bucket.to_string(), key.to_string()],
                &[],
                Vec::new(),
                None,
            )
            .await?;
        Self::require_success(&data, status)
    }
}

// MARK: - XML-parsning

/// Pull-baserad XML-parsning (`quick-xml`) av S3:s standardsvarsformat.
/// Delar samma element-vokabulär mellan riktig AWS S3 och Ceph RGW
/// (verifierat mot Hostups riktiga svar av Swift-sidan) — samma
/// SAX-liknande "spåra aktuell tagg + en `in_target`-flagga"-mönster som
/// `S3XMLParser` i Swift, fast med `quick-xml` istället för Foundations
/// `XMLParser`.
///
/// Sedan quick-xml 0.38 levereras entiteter inte längre färdigavkodade i
/// `Text` utan som separata `GeneralRef`-händelser. Den här löser upp en
/// sådan referens till sin text: teckenreferenser (`&#38;`, `&#x26;`) via
/// quick-xml, och de fem fördefinierade namnen själv. Okända namn ger tom
/// sträng — S3 använder bara de fördefinierade.
fn ref_text(e: &quick_xml::events::BytesRef) -> String {
    if let Ok(Some(c)) = e.resolve_char_ref() {
        return c.to_string();
    }
    match e.decode().unwrap_or_default().as_ref() {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "apos" => "'",
        "quot" => "\"",
        _ => "",
    }
    .to_string()
}

pub fn parse_buckets(data: &[u8]) -> Vec<S3Bucket> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_reader(data);
    let mut buf = Vec::new();
    let mut buckets = Vec::new();
    let mut in_bucket = false;
    let mut current_tag = String::new();
    let mut name: Option<String> = None;
    let mut date: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                current_tag = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if current_tag == "Bucket" {
                    in_bucket = true;
                    name = None;
                    date = None;
                }
            }
            Ok(Event::Text(e)) if in_bucket => {
                let text = e.xml10_content().map(|s| s.into_owned()).unwrap_or_default();
                match current_tag.as_str() {
                    "Name" => name = Some(name.unwrap_or_default() + &text),
                    "CreationDate" => date = Some(date.unwrap_or_default() + &text),
                    _ => {}
                }
            }
            Ok(Event::GeneralRef(e)) if in_bucket => {
                let text = ref_text(&e);
                match current_tag.as_str() {
                    "Name" => name = Some(name.unwrap_or_default() + &text),
                    "CreationDate" => date = Some(date.unwrap_or_default() + &text),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                current_tag.clear();
                if e.local_name().as_ref() == b"Bucket" {
                    if let Some(n) = name.take() {
                        buckets.push(S3Bucket {
                            name: n.trim().to_string(),
                            creation_date: date.take().map(|d| d.trim().to_string()),
                        });
                    }
                    in_bucket = false;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    buckets
}

pub fn parse_objects(data: &[u8]) -> Vec<S3Object> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_reader(data);
    let mut buf = Vec::new();
    let mut objects = Vec::new();
    let mut in_contents = false;
    let mut current_tag = String::new();
    let mut key: Option<String> = None;
    let mut size: Option<String> = None;
    let mut modified: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                current_tag = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if current_tag == "Contents" {
                    in_contents = true;
                    key = None;
                    size = None;
                    modified = None;
                }
            }
            Ok(Event::Text(e)) if in_contents => {
                let text = e.xml10_content().map(|s| s.into_owned()).unwrap_or_default();
                match current_tag.as_str() {
                    "Key" => key = Some(key.unwrap_or_default() + &text),
                    "Size" => size = Some(size.unwrap_or_default() + &text),
                    "LastModified" => modified = Some(modified.unwrap_or_default() + &text),
                    _ => {}
                }
            }
            Ok(Event::GeneralRef(e)) if in_contents => {
                let text = ref_text(&e);
                match current_tag.as_str() {
                    "Key" => key = Some(key.unwrap_or_default() + &text),
                    "Size" => size = Some(size.unwrap_or_default() + &text),
                    "LastModified" => modified = Some(modified.unwrap_or_default() + &text),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                current_tag.clear();
                if e.local_name().as_ref() == b"Contents" {
                    if let Some(k) = key.take() {
                        objects.push(S3Object {
                            key: k.trim().to_string(),
                            size: size
                                .take()
                                .and_then(|s| s.trim().parse().ok())
                                .unwrap_or(0),
                            last_modified: modified.take().map(|m| m.trim().to_string()),
                        });
                    }
                    in_contents = false;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    objects
}

pub fn parse_error(data: &[u8]) -> (String, String) {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_reader(data);
    let mut buf = Vec::new();
    let mut current_tag = String::new();
    let mut code = String::new();
    let mut message = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                current_tag = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
            }
            Ok(Event::Text(e)) => {
                let text = e.xml10_content().map(|s| s.into_owned()).unwrap_or_default();
                match current_tag.as_str() {
                    "Code" => code.push_str(&text),
                    "Message" => message.push_str(&text),
                    _ => {}
                }
            }
            Ok(Event::GeneralRef(e)) => {
                let text = ref_text(&e);
                match current_tag.as_str() {
                    "Code" => code.push_str(&text),
                    "Message" => message.push_str(&text),
                    _ => {}
                }
            }
            Ok(Event::End(_)) => current_tag.clear(),
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    let code = code.trim().to_string();
    let message = message.trim().to_string();
    (if code.is_empty() { "Unknown".to_string() } else { code }, message)
}

// MARK: - Sparade anslutningar

/// En namngiven, sparad S3-anslutning — samma "wrapper runt ren
/// datamodell"-mönster som `WireGuardProfile` runt sin `config`. Nycklarna
/// sparas i klartext i JSON-filen, precis som `WireGuardProfile` redan gör
/// för WireGuard-privatnycklar (samma medvetna v1-avgränsning, ingen
/// Keychain-motsvarighet på Linux än).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3Connection {
    pub id: Uuid,
    pub name: String,
    pub endpoint: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub modified_at: ReferenceDate,
}

impl S3Connection {
    pub fn new(name: String, endpoint: String, region: String, access_key_id: String, secret_access_key: String) -> Self {
        S3Connection {
            id: Uuid::new_v4(),
            name,
            endpoint,
            region,
            access_key_id,
            secret_access_key,
            modified_at: ReferenceDate::now(),
        }
    }

    pub fn credentials(&self) -> S3Credentials {
        S3Credentials {
            access_key_id: self.access_key_id.clone(),
            secret_access_key: self.secret_access_key.clone(),
        }
    }
}

/// Testar en sparad anslutning genom att lista buckets — samma "första
/// riktiga API-anrop bevisar att nycklar/endpoint faktiskt fungerar"-idé
/// som `key_deploy::deploy_and_verify`. Körs på en egen bakgrundstråd med
/// egen tokio-runtime: `reqwest`s async-metoder (liksom `russh`/`russh-
/// sftp` på andra ställen) kräver en tokio-reaktor, som GTK:s huvudloop
/// (`glib::spawn_future_local`) inte har — samma mönster som
/// `ssh::spawn_shell`/`wake_on_lan::spawn_send`.
pub fn spawn_test_connection(
    connection: S3Connection,
) -> async_channel::Receiver<Result<Vec<S3Bucket>, String>> {
    spawn(connection, |client| async move {
        client.list_buckets().await
    })
}

/// Kör `op` mot en nyansluten `S3Client` för `connection` på en egen
/// bakgrundstråd med egen tokio-runtime — det delade mönstret bakom ALLA
/// `spawn_*`-hjälpare nedan (och `spawn_test_connection` ovan). Samma skäl
/// som `ssh::spawn_shell`/`wake_on_lan::spawn_send`: `reqwest`s async-API
/// kräver en tokio-reaktor, som GTK:s huvudloop inte har.
fn spawn<F, Fut, T>(
    connection: S3Connection,
    op: F,
) -> async_channel::Receiver<Result<T, String>>
where
    F: FnOnce(S3Client) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<T, S3Error>>,
    T: Send + 'static,
{
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för s3-tråden");
        let result = rt.block_on(async move {
            let client = S3Client::new(&connection.endpoint, connection.region.clone(), connection.credentials())
                .map_err(|e| e.to_string())?;
            op(client).await.map_err(|e| e.to_string())
        });
        let _ = tx.send_blocking(result);
    });
    rx
}

pub fn spawn_list_buckets(connection: S3Connection) -> async_channel::Receiver<Result<Vec<S3Bucket>, String>> {
    spawn(connection, |client| async move { client.list_buckets().await })
}

pub fn spawn_create_bucket(connection: S3Connection, name: String) -> async_channel::Receiver<Result<(), String>> {
    spawn(connection, move |client| async move { client.create_bucket(&name).await })
}

pub fn spawn_delete_bucket(connection: S3Connection, name: String) -> async_channel::Receiver<Result<(), String>> {
    spawn(connection, move |client| async move { client.delete_bucket(&name).await })
}

pub fn spawn_list_objects(connection: S3Connection, bucket: String) -> async_channel::Receiver<Result<Vec<S3Object>, String>> {
    spawn(connection, move |client| async move { client.list_objects(&bucket, None).await })
}

pub fn spawn_put_object(
    connection: S3Connection,
    bucket: String,
    key: String,
    data: Vec<u8>,
) -> async_channel::Receiver<Result<(), String>> {
    spawn(connection, move |client| async move {
        client.put_object(&bucket, &key, data, None).await
    })
}

/// Laddar ner objektet DIREKT TILL FIL, strömmande — se
/// `S3Client::get_object_to_file` för varför det inte går via minnet.
pub fn spawn_download_object(
    connection: S3Connection,
    bucket: String,
    key: String,
    destination: std::path::PathBuf,
) -> async_channel::Receiver<Result<(), String>> {
    spawn(connection, move |client| async move {
        client.get_object_to_file(&bucket, &key, &destination).await
    })
}

pub fn spawn_delete_object(
    connection: S3Connection,
    bucket: String,
    key: String,
) -> async_channel::Receiver<Result<(), String>> {
    spawn(connection, move |client| async move { client.delete_object(&bucket, &key).await })
}

/// Persistent S3-anslutningsdatabas, `~/.bastion/s3connections.json` —
/// samma mönster som `WireGuardProfileStore`/`SnippetStore`.
pub struct S3ConnectionStore {
    path: std::path::PathBuf,
    connections: Vec<S3Connection>,
}

impl S3ConnectionStore {
    pub fn default_path() -> std::path::PathBuf {
        dirs::home_dir()
            .expect("kunde inte hitta hemkatalogen")
            .join(".bastion/s3connections.json")
    }

    pub fn open(path: std::path::PathBuf) -> std::io::Result<Self> {
        let connections = match std::fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{}: {e}", path.display()),
                )
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(S3ConnectionStore { path, connections })
    }

    pub fn all(&self) -> Vec<&S3Connection> {
        let mut c: Vec<&S3Connection> = self.connections.iter().collect();
        c.sort_by_key(|x| x.name.to_lowercase());
        c
    }

    /// Inte anropad av UI:t än (raderna i `main.rs` klonar direkt ur
    /// `all()` istället) — kvar för symmetri med resten av store-API:t
    /// (`WireGuardProfileStore`/`HostStore` har motsvarande) och testad
    /// direkt.
    #[allow(dead_code)]
    pub fn get(&self, id: Uuid) -> Option<&S3Connection> {
        self.connections.iter().find(|c| c.id == id)
    }

    pub fn upsert(&mut self, mut connection: S3Connection) -> std::io::Result<()> {
        connection.modified_at = ReferenceDate::now();
        if let Some(existing) = self.connections.iter_mut().find(|c| c.id == connection.id) {
            *existing = connection;
        } else {
            self.connections.push(connection);
        }
        self.persist()
    }

    pub fn delete(&mut self, id: Uuid) -> std::io::Result<()> {
        self.connections.retain(|c| c.id != id);
        self.persist()
    }

    fn persist(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let mut sorted = self.connections.clone();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        crate::fsutil::atomic_write(&self.path, serde_json::to_string_pretty(&sorted)?.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixerad, icke tidsberoende SigV4-vektor — EXAKT samma som Swift-
    /// sidans `testSigV4MatchesVerifiedReferenceVector`, härledd ur en
    /// oberoende Python-referensimplementation som fick ett genuint 200 OK
    /// mot Hostups riktiga S3-kompatibla tjänst. Om den här porten ger
    /// SAMMA `Authorization`-header för samma indata är algoritmen bevisat
    /// korrekt, inte bara "ser rimlig ut".
    #[test]
    fn sigv4_matches_verified_reference_vector() {
        let credentials = S3Credentials {
            access_key_id: "AKIDEXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
        };
        let signed = sign(
            "GET",
            "examplebucket.s3.hostup.se",
            "/test.txt",
            "",
            b"",
            "us-east-1",
            &credentials,
            "20260101T000000Z",
        );

        assert_eq!(
            signed.content_sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            signed.authorization_header,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20260101/us-east-1/s3/aws4_request, \
             SignedHeaders=host;x-amz-content-sha256;x-amz-date, \
             Signature=2cf4e62d28a10475635f645779da044490aebcce8d9475e44a59523e179c5785"
        );
    }

    #[test]
    fn sigv4_differs_with_different_payload() {
        let credentials = S3Credentials {
            access_key_id: "AKID".to_string(),
            secret_access_key: "secret".to_string(),
        };
        let empty = sign("PUT", "h", "/x", "", b"", "us-east-1", &credentials, "20260101T000000Z");
        let non_empty = sign(
            "PUT", "h", "/x", "", "hej".as_bytes(), "us-east-1", &credentials, "20260101T000000Z",
        );
        assert_ne!(empty.authorization_header, non_empty.authorization_header);
        assert_ne!(empty.content_sha256, non_empty.content_sha256);
    }

    // MARK: - XML-parsning (riktiga svarsformat, fångade från Hostups tjänst)

    const REAL_LIST_BUCKETS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?><ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Owner><ID>client_9455$main</ID><DisplayName>Client 9455</DisplayName></Owner><Buckets><Bucket><Name>bastion-test</Name><CreationDate>2026-07-07T09:30:00.000Z</CreationDate></Bucket></Buckets></ListAllMyBucketsResult>"#;

    #[test]
    fn parses_real_list_buckets_response() {
        let buckets = parse_buckets(REAL_LIST_BUCKETS_XML.as_bytes());
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name, "bastion-test");
        assert!(buckets[0].creation_date.is_some());
    }

    const REAL_LIST_OBJECTS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?><ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Name>bastion-test</Name><Contents><Key>hello.txt</Key><LastModified>2026-07-07T09:31:00.000Z</LastModified><Size>5</Size></Contents></ListBucketResult>"#;

    #[test]
    fn parses_list_objects_response() {
        let objects = parse_objects(REAL_LIST_OBJECTS_XML.as_bytes());
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].key, "hello.txt");
        assert_eq!(objects[0].size, 5);
    }

    const REAL_ERROR_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?><Error><Code>NoSuchBucket</Code><Message>The specified bucket does not exist.</Message></Error>"#;

    #[test]
    fn parses_error_response() {
        let (code, message) = parse_error(REAL_ERROR_XML.as_bytes());
        assert_eq!(code, "NoSuchBucket");
        assert_eq!(message, "The specified bucket does not exist.");
    }

    /// quick-xml 0.38 slutade leverera entiteter färdigavkodade i `Text` och
    /// rapporterar dem som separata `GeneralRef`-händelser i stället. Utan de
    /// arm:arna tappas varje `&`, `<` och `>` tyst ur objektnycklar och
    /// felmeddelanden — bara `&`-tecknet är fullt lagligt i en S3-nyckel.
    #[test]
    fn resolves_entities_split_out_of_text_events() {
        let objects =
            parse_objects(br#"<R><Contents><Key>a &amp; b/c&#38;d.txt</Key><Size>7</Size></Contents></R>"#);
        assert_eq!(objects[0].key, "a & b/c&d.txt");

        let (_, message) =
            parse_error(br#"<Error><Code>X</Code><Message>a &lt; b &amp; c &gt; d</Message></Error>"#);
        assert_eq!(message, "a < b & c > d");
    }

    /// Indenterad XML får inte läcka in radbrytningar i värdena. Reader:ns
    /// `trim_text` går inte att använda längre — den trimmar varje fragment
    /// för sig, så mellanrummen runt en entitet skulle försvinna. Därför
    /// trimmas i stället det färdiga värdet.
    #[test]
    fn ignores_whitespace_between_elements() {
        let buckets = parse_buckets(
            br#"<L>
  <Buckets>
    <Bucket>
      <Name>x</Name>
      <CreationDate>2020-01-01T00:00:00Z</CreationDate>
    </Bucket>
  </Buckets>
</L>"#,
        );
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name, "x");
        assert_eq!(buckets[0].creation_date.as_deref(), Some("2020-01-01T00:00:00Z"));
    }

    // MARK: - Regressionstester för CodeRabbit-fyndet (PR #90) — porterade

    #[test]
    fn host_includes_non_default_port() {
        let client = S3Client::new(
            "http://localhost:9000",
            "us-east-1".to_string(),
            S3Credentials { access_key_id: "AKID".to_string(), secret_access_key: "secret".to_string() },
        )
        .unwrap();
        assert_eq!(client.host(), "localhost:9000");
    }

    #[test]
    fn host_omits_default_https_port() {
        let client = S3Client::new(
            "https://s3.hostup.se:443",
            "us-east-1".to_string(),
            S3Credentials { access_key_id: "AKID".to_string(), secret_access_key: "secret".to_string() },
        )
        .unwrap();
        assert_eq!(client.host(), "s3.hostup.se");
    }

    #[test]
    fn host_omits_when_no_port_specified() {
        let client = S3Client::new(
            "https://s3.hostup.se",
            "us-east-1".to_string(),
            S3Credentials { access_key_id: "AKID".to_string(), secret_access_key: "secret".to_string() },
        )
        .unwrap();
        assert_eq!(client.host(), "s3.hostup.se");
    }

    // MARK: - Genuin end-to-end mot en riktig, minimal HTTP-server

    /// Minimal, fristående HTTP/1.1-server (rå TCP, inget ramverk) som
    /// beter sig som en S3-kompatibel tjänst: läser en begäran, spelar in
    /// dess metod/väg/headers, svarar med ett fördefinierat XML-svar.
    /// Bevisar att HELA vägen — URL-byggande, signering, riktiga HTTP-
    /// anrop via `reqwest`, XML-tolkning av svaret — fungerar ihop, inte
    /// bara `sign`/`parse_*` isolerat.
    async fn spawn_fake_s3_server(
        response_body: &'static str,
    ) -> (u16, tokio::sync::oneshot::Receiver<String>) {
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
                "HTTP/1.1 200 OK\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = socket.write_all(body.as_bytes()).await;
            let _ = socket.shutdown().await;
            let _ = tx.send(request_head);
        });
        (port, rx)
    }

    #[tokio::test]
    async fn list_buckets_sends_a_correctly_signed_request_and_parses_the_response() {
        let (port, rx) = spawn_fake_s3_server(REAL_LIST_BUCKETS_XML).await;
        let client = S3Client::new(
            &format!("http://127.0.0.1:{port}"),
            "us-east-1".to_string(),
            S3Credentials { access_key_id: "AKID".to_string(), secret_access_key: "secret".to_string() },
        )
        .unwrap();

        let buckets = client.list_buckets().await.expect("list_buckets misslyckades");
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name, "bastion-test");

        let request_head = rx.await.expect("servern fick aldrig någon begäran");
        assert!(request_head.starts_with("GET / HTTP/1.1"), "fel begäranrad: {request_head}");
        // HTTP-headernamn skiftlägesokänsliga; `reqwest`/`hyper` skriver dem
        // i gemener på tråden (verifierat empiriskt, inte antaget).
        assert!(request_head.contains("authorization: AWS4-HMAC-SHA256 "), "saknar Authorization-header: {request_head}");
        assert!(request_head.contains(&format!("host: 127.0.0.1:{port}")), "Host-headern matchar inte den signerade: {request_head}");
    }

    #[tokio::test]
    async fn put_object_sends_the_body_and_content_type_header() {
        let (port, rx) = spawn_fake_s3_server("").await;
        let client = S3Client::new(
            &format!("http://127.0.0.1:{port}"),
            "us-east-1".to_string(),
            S3Credentials { access_key_id: "AKID".to_string(), secret_access_key: "secret".to_string() },
        )
        .unwrap();

        client
            .put_object("mybucket", "hello.txt", b"hej".to_vec(), Some("text/plain"))
            .await
            .expect("put_object misslyckades");

        let request_head = rx.await.expect("servern fick aldrig någon begäran");
        assert!(request_head.starts_with("PUT /mybucket/hello.txt HTTP/1.1"), "fel begäranrad: {request_head}");
        assert!(request_head.contains("content-type: text/plain"), "saknar Content-Type-header: {request_head}");
        assert!(request_head.ends_with("hej"), "kroppen skickades inte med: {request_head}");
    }

    /// Strömmar ner ett objekt som är STÖRRE än någon rimlig buffert och
    /// verifierar att varenda byte kom fram korrekt — bevisar att
    /// bit-för-bit-skrivningen sätter ihop filen rätt, inte bara att den
    /// första biten råkar stämma.
    #[tokio::test]
    async fn get_object_to_file_streams_a_large_body_to_disk_correctly() {
        // ~3 MiB av ett upprepat mönster: stort nog att garanterat delas
        // upp i flera TCP-segment/chunks, med ett innehåll där en
        // felaktigt hopsatt fil (tappad eller omkastad bit) syns direkt.
        let payload: Vec<u8> = (0..3 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let expected = payload.clone();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let _ = socket.read(&mut buf).await;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(&payload).await;
            let _ = socket.shutdown().await;
        });

        let client = S3Client::new(
            &format!("http://127.0.0.1:{port}"),
            "us-east-1".to_string(),
            S3Credentials { access_key_id: "AKID".to_string(), secret_access_key: "secret".to_string() },
        )
        .unwrap();

        let dir = std::env::temp_dir().join(format!("bastion-s3-stream-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let destination = dir.join("stor-fil.bin");

        client
            .get_object_to_file("mybucket", "stor-fil.bin", &destination)
            .await
            .expect("strömmande nedladdning misslyckades");

        let written = std::fs::read(&destination).unwrap();
        assert_eq!(written.len(), expected.len(), "fel filstorlek");
        assert_eq!(written, expected, "innehållet sattes inte ihop korrekt");

        // Ingen kvarlämnad `.part`-fil efter en lyckad nedladdning.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".part"))
            .collect();
        assert!(leftovers.is_empty(), "temporära filer lämnades kvar: {leftovers:?}");

        std::fs::remove_dir_all(dir).ok();
    }

    /// Ett felsvar (t.ex. 404) ska ge ett tydligt fel OCH inte lämna
    /// någon fil alls på målsökvägen — varken en tom eller en halv.
    #[tokio::test]
    async fn get_object_to_file_writes_nothing_when_the_server_returns_an_error() {
        let (port, _rx) = spawn_fake_s3_server_with_status(
            "HTTP/1.1 404 Not Found",
            "<Error><Code>NoSuchKey</Code><Message>finns inte</Message></Error>",
        )
        .await;
        let client = S3Client::new(
            &format!("http://127.0.0.1:{port}"),
            "us-east-1".to_string(),
            S3Credentials { access_key_id: "AKID".to_string(), secret_access_key: "secret".to_string() },
        )
        .unwrap();

        let dir = std::env::temp_dir().join(format!("bastion-s3-stream-err-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let destination = dir.join("finns-inte.bin");

        let err = client
            .get_object_to_file("mybucket", "finns-inte.bin", &destination)
            .await
            .expect_err("ett 404-svar ska ge Err");
        match err {
            S3Error::HttpError { status, ref code, .. } => {
                assert_eq!(status, 404);
                assert_eq!(code, "NoSuchKey");
            }
            other => panic!("fel feltyp: {other:?}"),
        }

        assert!(!destination.exists(), "ingen fil ska ha skapats vid ett felsvar");
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).collect();
        assert!(leftovers.is_empty(), "katalogen ska vara tom, fick {} poster", leftovers.len());

        std::fs::remove_dir_all(dir).ok();
    }

    /// Som `spawn_fake_s3_server`, fast med valfri statusrad.
    async fn spawn_fake_s3_server_with_status(
        status_line: &'static str,
        response_body: &'static str,
    ) -> (u16, tokio::sync::oneshot::Receiver<String>) {
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
                "{status_line}\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = socket.write_all(body.as_bytes()).await;
            let _ = socket.shutdown().await;
            let _ = tx.send(request_head);
        });
        (port, rx)
    }

    fn make_connection() -> S3Connection {
        S3Connection::new(
            "Hemma".to_string(),
            "https://s3.hostup.se".to_string(),
            "us-east-1".to_string(),
            "AKID".to_string(),
            "secret".to_string(),
        )
    }

    #[test]
    fn connection_store_upsert_get_delete_sorted() {
        let dir = std::env::temp_dir().join(format!("bastion-s3-test-{}", Uuid::new_v4()));
        let mut store = S3ConnectionStore::open(dir.join("s3connections.json")).unwrap();
        let mut a = make_connection();
        a.name = "Hemma".to_string();
        let mut b = make_connection();
        b.name = "jobbet".to_string();
        let a_id = a.id;
        let b_id = b.id;
        store.upsert(a).unwrap();
        store.upsert(b).unwrap();

        let names: Vec<&str> = store.all().iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Hemma", "jobbet"]); // skiftlägesokänslig sort
        assert_eq!(store.get(a_id).unwrap().access_key_id, "AKID");

        store.delete(b_id).unwrap();
        let names: Vec<&str> = store.all().iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Hemma"]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn connection_store_persists_across_instances() {
        let dir = std::env::temp_dir().join(format!("bastion-s3-test-{}", Uuid::new_v4()));
        let path = dir.join("s3connections.json");
        let connection = make_connection();
        let id = connection.id;
        {
            let mut s1 = S3ConnectionStore::open(path.clone()).unwrap();
            s1.upsert(connection).unwrap();
        }
        let s2 = S3ConnectionStore::open(path).unwrap();
        assert_eq!(s2.get(id).unwrap().name, "Hemma");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_corrupt_s3connections_json_is_an_error_not_a_silent_empty_state() {
        let dir = std::env::temp_dir().join(format!("bastion-s3-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s3connections.json");
        std::fs::write(&path, "{ inte giltig json").unwrap();

        let result = S3ConnectionStore::open(path);
        assert!(
            result.is_err(),
            "en trunkerad/skadad fil ska propagera ett fel, inte tyst bli en tom lista"
        );
        std::fs::remove_dir_all(dir).ok();
    }
}
