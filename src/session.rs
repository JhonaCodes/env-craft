use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use dryoc::classic::crypto_box::{
    PublicKey, SecretKey, crypto_box_keypair, crypto_box_seal, crypto_box_seal_open,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::AppConfig;

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
        let path = AppConfig::requests_dir()?.join(format!("{}.json", self.request_id));
        fs::create_dir_all(AppConfig::requests_dir()?)?;
        fs::write(&path, serde_json::to_vec_pretty(&stored)?)?;
        Ok(path)
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
    use super::{DeliverySession, encrypt_for_session};

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
}
