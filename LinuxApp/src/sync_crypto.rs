//! Port av Sources/SSHCore/SyncCrypto.swift. End-to-end-kryptering av
//! synktillståndet — allt krypteras på enheten innan det lämnar den, så
//! vilken molntjänst filen än hamnar i (Dropbox/Google Drive/OneDrive) ser
//! bara chiffertext. Nyckeln härleds ur en lösenfras med PBKDF2-HMAC-SHA256,
//! nyttolasten skyddas med AES-256-GCM (autentiserad).
//!
//! Kuvertformat (IDENTISKT med Swift-sidan, verifierat empiriskt — inte
//! gissat): `"BSYNC1" | iterationer(u32 BE) | salt(16) | AES-GCM combined`,
//! där "combined" är `nonce(12) || ciphertext || tag(16)` — samma layout
//! som Apple CryptoKits `AES.GCM.SealedBox.combined`.

use crate::host::SyncState;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::Rng;
use sha2::Sha256;

pub const DEFAULT_ITERATIONS: u32 = 210_000;
const MAGIC: &[u8] = b"BSYNC1";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

#[derive(Debug, PartialEq)]
pub enum SyncCryptoError {
    BadFormat,
    WrongPassphraseOrTampered,
}

impl std::fmt::Display for SyncCryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncCryptoError::BadFormat => write!(f, "ogiltigt kuvertformat"),
            SyncCryptoError::WrongPassphraseOrTampered => write!(f, "fel lösenfras eller manipulerad data"),
        }
    }
}

fn derive_key(passphrase: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, iterations, &mut key);
    key
}

pub fn seal(state: &SyncState, passphrase: &str, iterations: u32) -> Result<Vec<u8>, SyncCryptoError> {
    let mut salt = [0u8; SALT_LEN];
    rand::rng().fill_bytes(&mut salt);
    let key = derive_key(passphrase, &salt, iterations);

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::try_from(nonce_bytes.as_slice()).expect("12 bytes");

    let plaintext = serde_json::to_vec(state).map_err(|_| SyncCryptoError::BadFormat)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| SyncCryptoError::BadFormat)?;
    let ciphertext_and_tag = cipher.encrypt(&nonce, plaintext.as_ref()).map_err(|_| SyncCryptoError::BadFormat)?;

    let mut out = Vec::with_capacity(MAGIC.len() + 4 + SALT_LEN + NONCE_LEN + ciphertext_and_tag.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&iterations.to_be_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext_and_tag);
    Ok(out)
}

pub fn open(data: &[u8], passphrase: &str) -> Result<SyncState, SyncCryptoError> {
    let header_len = MAGIC.len() + 4 + SALT_LEN + NONCE_LEN;
    if data.len() <= header_len || &data[..MAGIC.len()] != MAGIC {
        return Err(SyncCryptoError::BadFormat);
    }
    let iterations = u32::from_be_bytes(data[MAGIC.len()..MAGIC.len() + 4].try_into().unwrap());
    let salt = &data[MAGIC.len() + 4..MAGIC.len() + 4 + SALT_LEN];
    let nonce_bytes = &data[MAGIC.len() + 4 + SALT_LEN..header_len];
    let ciphertext_and_tag = &data[header_len..];

    let key = derive_key(passphrase, salt, iterations);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| SyncCryptoError::BadFormat)?;
    let nonce = Nonce::try_from(nonce_bytes).expect("12 bytes");
    let plaintext = cipher
        .decrypt(&nonce, ciphertext_and_tag)
        .map_err(|_| SyncCryptoError::WrongPassphraseOrTampered)?;
    serde_json::from_slice(&plaintext).map_err(|_| SyncCryptoError::WrongPassphraseOrTampered)
}

/// Krypterad variant av `FolderSyncProvider` — samma mapp-transport, men
/// filen är AES-GCM-krypterad med en lösenfras. Den man vill använda mot
/// en tredjeparts molnmapp (Dropbox/Drive/OneDrive), till skillnad från
/// den oformaterade `FolderSyncProvider` som bara passar en redan betrodd
/// lokal/synkad mapp.
pub struct EncryptedFolderSyncProvider {
    path: std::path::PathBuf,
    passphrase: String,
    iterations: u32,
}

impl EncryptedFolderSyncProvider {
    pub fn new(path: std::path::PathBuf, passphrase: String) -> Self {
        Self { path, passphrase, iterations: DEFAULT_ITERATIONS }
    }
}

impl crate::sync::SyncProvider for EncryptedFolderSyncProvider {
    fn pull(&self) -> std::io::Result<Option<SyncState>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&self.path)?;
        let state = open(&data, &self.passphrase)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(Some(state))
    }

    fn push(&self, state: &SyncState) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let sealed = seal(state, &self.passphrase, self.iterations)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        std::fs::write(&self.path, sealed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Samma kända testvektorer som Swift-sidans
    /// `testPBKDF2KnownAnswerVectors` (password="password", salt="salt",
    /// keyLength=32) — verifierar BYTE-IDENTISKA resultat mot referensen,
    /// inte bara "ser rimligt ut".
    #[test]
    fn pbkdf2_known_answer_vectors_match_swift_reference() {
        let mut out = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(b"password", b"salt", 1, &mut out);
        assert_eq!(to_hex(&out), "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b");

        let mut out2 = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(b"password", b"salt", 2, &mut out2);
        assert_eq!(to_hex(&out2), "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43");

        let mut out3 = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(b"password", b"salt", 4096, &mut out3);
        assert_eq!(to_hex(&out3), "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a");
    }

    #[test]
    fn seal_open_round_trip() {
        let mut state = SyncState::default();
        state.hosts.push(crate::host::Host::new("test".into(), "h".into(), "u".into()));
        let sealed = seal(&state, "korrekt lösenfras", 1000).unwrap();
        let opened = open(&sealed, "korrekt lösenfras").unwrap();
        assert_eq!(opened.hosts.len(), 1);
        assert_eq!(opened.hosts[0].alias, "test");
    }

    #[test]
    fn wrong_passphrase_fails() {
        let state = SyncState::default();
        let sealed = seal(&state, "rätt", 1000).unwrap();
        let result = open(&sealed, "fel");
        assert_eq!(result.unwrap_err(), SyncCryptoError::WrongPassphraseOrTampered);
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let state = SyncState::default();
        let mut sealed = seal(&state, "lösen", 1000).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;
        let result = open(&sealed, "lösen");
        assert_eq!(result.unwrap_err(), SyncCryptoError::WrongPassphraseOrTampered);
    }

    // TILLFÄLLIG cross-språk-verifiering mot Tests/SSHCoreTests/SyncCryptoTests.swift
    // — tas bort igen efter körning.
    #[test]
    fn bad_format_is_rejected_cleanly() {
        assert_eq!(open(b"not a valid envelope", "x").unwrap_err(), SyncCryptoError::BadFormat);
    }

    #[test]
    fn ciphertext_leaks_no_plaintext() {
        let mut state = SyncState::default();
        let mut host = crate::host::Host::new("web".into(), "10.0.0.5".into(), "deploy".into());
        host.tags = vec!["prod".to_string()];
        state.hosts.push(host);
        let sealed = seal(&state, "pw", 1000).unwrap();
        let text = String::from_utf8_lossy(&sealed);
        assert!(!text.contains("10.0.0.5"));
        assert!(!text.contains("deploy"));
        assert!(!text.contains("web"));
    }

    /// Två oberoende HostStores synkar genom en KRYPTERAD delad fil och
    /// konvergerar — samma cross-instans-verifiering som den oformaterade
    /// `FolderSyncProvider` i sync.rs, nu med kryptering i vägen. En tredje
    /// enhet med FEL lösenfras kan inte läsa alls.
    #[test]
    fn encrypted_provider_converges_and_rejects_wrong_passphrase() {
        use crate::host::{Host, HostStore};
        use crate::sync::SyncProvider;

        let dir = std::env::temp_dir().join(format!("bastion-enc-test-{}", uuid::Uuid::new_v4()));
        let shared_path = dir.join("shared.enc");

        let mut store_a = HostStore::open(dir.join("a/hosts.json")).unwrap();
        let mut store_b = HostStore::open(dir.join("b/hosts.json")).unwrap();

        let provider = EncryptedFolderSyncProvider::new(shared_path.clone(), "delad-hemlis".to_string());
        store_a.upsert(Host::new("nas".into(), "10.0.0.2".into(), "root".into())).unwrap();
        store_a.sync(&provider).unwrap();
        store_b.sync(&provider).unwrap();

        assert_eq!(store_b.all()[0].alias, "nas");

        let wrong_provider = EncryptedFolderSyncProvider::new(shared_path, "gissning".to_string());
        assert!(wrong_provider.pull().is_err(), "fel lösenfras borde inte kunna läsa den krypterade filen");

        std::fs::remove_dir_all(dir).ok();
    }
}
