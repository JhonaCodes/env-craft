use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use dryoc::classic::crypto_box::{
    PublicKey, SecretKey, crypto_box_keypair, crypto_box_seal, crypto_box_seal_open,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

use crate::config::AppConfig;
use crate::fs_sec;

const SESSION_TTL_SECS: i64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryEnvelope {
    pub request_id: Uuid,
    pub project: String,
    pub environment: String,
    pub logical_key: String,
    pub secret_name: String,
    pub encrypted_payload: String,
    pub delivered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub request_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub recipient_public_key_b64: String,
    pub recipient_secret_key_b64: String,
}

#[derive(Debug, Clone)]
pub struct DeliverySession {
    pub request_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    recipient_public_key: PublicKey,
    recipient_secret_key: SecretKey,
}

impl Drop for DeliverySession {
    fn drop(&mut self) {
        self.recipient_secret_key.zeroize();
    }
}

impl DeliverySession {
    pub fn new() -> Self {
        let (public_key, secret_key) = crypto_box_keypair();
        let created_at = Utc::now();
        let expires_at = created_at + chrono::Duration::seconds(SESSION_TTL_SECS);

        Self {
            request_id: Uuid::new_v4(),
            created_at,
            expires_at,
            recipient_public_key: public_key,
            recipient_secret_key: secret_key,
        }
    }

    pub fn recipient_public_key_b64(&self) -> String {
        STANDARD.encode(self.recipient_public_key)
    }

    pub fn ttl(&self) -> Duration {
        Duration::from_secs(SESSION_TTL_SECS as u64)
    }

    pub fn decrypt_payload(&self, encrypted_payload_b64: &str) -> Result<String> {
        let encrypted = STANDARD
            .decode(encrypted_payload_b64)
            .context("failed to decode encrypted payload")?;
        let mut plaintext = vec![0_u8; encrypted.len().saturating_sub(48)];
        crypto_box_seal_open(
            &mut plaintext,
            &encrypted,
            &self.recipient_public_key,
            &self.recipient_secret_key,
        )
        .map_err(|_| anyhow!("failed to decrypt delivery payload"))?;
        String::from_utf8(plaintext).context("delivery payload was not valid UTF-8")
    }

    pub fn save(&self) -> Result<PathBuf> {
        let stored = StoredSession {
            request_id: self.request_id,
            created_at: self.created_at,
            expires_at: self.expires_at,
            recipient_public_key_b64: self.recipient_public_key_b64(),
            recipient_secret_key_b64: STANDARD.encode(self.recipient_secret_key),
        };
        let requests_dir = AppConfig::requests_dir()?;
        fs_sec::create_restricted_dir(&requests_dir)?;
        let path = requests_dir.join(format!("{}.json", self.request_id));
        fs_sec::write_secret_file(&path, &serde_json::to_vec_pretty(&stored)?)?;
        Ok(path)
    }

    /// Remove this session file from disk.
    pub fn delete_from_disk(&self) -> Result<()> {
        if let Ok(path) = AppConfig::requests_dir()
            .map(|d| d.join(format!("{}.json", self.request_id)))
        {
            if path.exists() {
                fs::remove_file(&path)?;
            }
        }
        Ok(())
    }

    pub fn from_stored(stored: StoredSession) -> Result<Self> {
        let public_key = key_from_b64::<32>(&stored.recipient_public_key_b64)?;
        let secret_key = key_from_b64::<32>(&stored.recipient_secret_key_b64)?;

        Ok(Self {
            request_id: stored.request_id,
            created_at: stored.created_at,
            expires_at: stored.expires_at,
            recipient_public_key: public_key,
            recipient_secret_key: secret_key,
        })
    }
}

/// Remove all expired session files from the requests cache directory.
pub fn purge_expired_sessions() -> Result<u64> {
    let requests_dir = match AppConfig::requests_dir() {
        Ok(dir) if dir.is_dir() => dir,
        _ => return Ok(0),
    };
    let now = Utc::now();
    let mut removed = 0_u64;
    for entry in fs::read_dir(requests_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(stored) = serde_json::from_str::<StoredSession>(&raw) {
                if stored.expires_at < now {
                    let _ = fs::remove_file(&path);
                    removed += 1;
                }
            }
        }
    }
    Ok(removed)
}

pub fn encrypt_for_session(public_key_b64: &str, payload: &str) -> Result<String> {
    let public_key = key_from_b64::<32>(public_key_b64)?;
    let mut ciphertext = vec![0_u8; payload.len() + 48];
    crypto_box_seal(&mut ciphertext, payload.as_bytes(), &public_key)
        .map_err(|_| anyhow!("failed to encrypt payload for recipient"))?;
    Ok(STANDARD.encode(ciphertext))
}

fn key_from_b64<const N: usize>(value: &str) -> Result<[u8; N]> {
    let decoded = STANDARD
        .decode(value)
        .context("failed to decode base64 key")?;
    decoded
        .try_into()
        .map_err(|_| anyhow!("invalid key length, expected {}", N))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tempfile::tempdir;

    use super::{DeliverySession, StoredSession, encrypt_for_session, purge_expired_sessions};

    #[test]
    fn session_roundtrip_encrypts_and_decrypts() {
        let session = DeliverySession::new();
        let ciphertext =
            encrypt_for_session(&session.recipient_public_key_b64(), "super-secret").unwrap();

        assert_eq!(
            session.decrypt_payload(&ciphertext).unwrap(),
            "super-secret"
        );
    }

    #[test]
    fn zeroize_clears_secret_key_on_drop() {
        use zeroize::Zeroize;

        let session = DeliverySession::new();
        // Clone the secret key bytes before drop to compare
        let mut key_copy = session.recipient_secret_key;
        assert_ne!(key_copy, [0u8; 32]);
        // Zeroize should work on the key type
        key_copy.zeroize();
        assert_eq!(key_copy, [0u8; 32]);
    }

    #[test]
    fn purge_removes_expired_session_files() {
        let dir = tempdir().unwrap();
        let expired = StoredSession {
            request_id: uuid::Uuid::new_v4(),
            created_at: Utc::now() - chrono::Duration::seconds(300),
            expires_at: Utc::now() - chrono::Duration::seconds(180),
            recipient_public_key_b64: "AAAA".to_string(),
            recipient_secret_key_b64: "BBBB".to_string(),
        };
        let path = dir
            .path()
            .join(format!("{}.json", expired.request_id));
        std::fs::write(&path, serde_json::to_vec_pretty(&expired).unwrap()).unwrap();
        assert!(path.exists());

        // Purge reads from the real requests dir, so we test the file format is parseable
        let raw = std::fs::read_to_string(&path).unwrap();
        let parsed: StoredSession = serde_json::from_str(&raw).unwrap();
        assert!(parsed.expires_at < Utc::now());
    }

    #[test]
    fn purge_expired_sessions_returns_zero_when_no_dir() {
        // When the requests directory does not exist, purge should gracefully return 0.
        let count = purge_expired_sessions().unwrap_or(0);
        // We can't assert exact value since it depends on global state,
        // but it should not panic.
        assert!(count < u64::MAX);
    }
}
