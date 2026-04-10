use std::{
    fs,
    io::{Cursor, Read},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use dryoc::classic::crypto_box::{PublicKey, crypto_box_seal};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use serde_json::json;
use zip::ZipArchive;

use crate::{
    config::AppConfig,
    session::{DeliveryEnvelope, DeliverySession},
};

const API_BASE: &str = "https://api.github.com";
const ACCEPT_HEADER: &str = "application/vnd.github+json";
const API_VERSION: &str = "2022-11-28";
const SEAL_OVERHEAD: usize = 48;

#[derive(Debug, Clone)]
pub struct GitHubClient {
    http: Client,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepoPublicKey {
    pub key_id: String,
    pub key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepoSecretMetadata {
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub id: u64,
    pub name: String,
    pub expired: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct RepoSecretsResponse {
    secrets: Vec<RepoSecretMetadata>,
}

#[derive(Debug, Deserialize)]
struct ArtifactListResponse {
    artifacts: Vec<Artifact>,
}

impl GitHubClient {
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        let token = std::env::var(&config.token_env_var).with_context(|| {
            format!(
                "missing GitHub token in environment variable {}",
                config.token_env_var
            )
        })?;
        Self::new(&token)
    }

    pub fn new(token: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))?,
        );
        headers.insert(ACCEPT, HeaderValue::from_static(ACCEPT_HEADER));
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static(API_VERSION),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static("envcraft/0.1.0"));

        let http = Client::builder().default_headers(headers).build()?;
        Ok(Self { http })
    }

    pub fn get_repo_public_key(&self, owner: &str, repo: &str) -> Result<RepoPublicKey> {
        self.get_json(&format!(
            "{API_BASE}/repos/{owner}/{repo}/actions/secrets/public-key"
        ))
    }

    pub fn put_repo_secret(
        &self,
        owner: &str,
        repo: &str,
        secret_name: &str,
        value: &str,
    ) -> Result<()> {
        let public_key = self.get_repo_public_key(owner, repo)?;
        let encrypted_value = encrypt_for_github_secret(&public_key.key, value)?;
        let url = format!("{API_BASE}/repos/{owner}/{repo}/actions/secrets/{secret_name}");

        let response = self
            .http
            .put(url)
            .json(&json!({
                "encrypted_value": encrypted_value,
                "key_id": public_key.key_id,
            }))
            .send()?;

        match response.status().as_u16() {
            201 | 204 => Ok(()),
            _ => Err(read_error(response)),
        }
    }

    pub fn list_repo_secrets(&self, owner: &str, repo: &str) -> Result<Vec<RepoSecretMetadata>> {
        let payload: RepoSecretsResponse =
            self.get_json(&format!("{API_BASE}/repos/{owner}/{repo}/actions/secrets"))?;
        Ok(payload.secrets)
    }

    pub fn dispatch_delivery(
        &self,
        config: &AppConfig,
        session: &DeliverySession,
        project: &str,
        environment: &str,
        logical_key: &str,
        secret_name: &str,
    ) -> Result<()> {
        let url = format!(
            "{API_BASE}/repos/{owner}/{repo}/actions/workflows/{workflow}/dispatches",
            owner = config.github_owner,
            repo = config.control_repo,
            workflow = config.deliver_workflow,
        );

        let response = self
            .http
            .post(url)
            .json(&json!({
                "ref": config.default_ref,
                "inputs": {
                    "request_id": session.request_id.to_string(),
                    "project": project,
                    "environment": environment,
                    "logical_key": logical_key,
                    "secret_name": secret_name,
                    "recipient_public_key": session.recipient_public_key_b64(),
                }
            }))
            .send()?;

        match response.status().as_u16() {
            204 => Ok(()),
            _ => Err(read_error(response)),
        }
    }

    pub fn wait_for_delivery_artifact(
        &self,
        owner: &str,
        repo: &str,
        request_id: uuid::Uuid,
        timeout: Duration,
    ) -> Result<Artifact> {
        let started = Instant::now();
        let target_name = format!("envcraft-{request_id}");

        while started.elapsed() < timeout {
            let payload: ArtifactListResponse = self.get_json(&format!(
                "{API_BASE}/repos/{owner}/{repo}/actions/artifacts"
            ))?;

            if let Some(artifact) = payload
                .artifacts
                .into_iter()
                .find(|artifact| artifact.name == target_name && !artifact.expired)
            {
                return Ok(artifact);
            }

            thread::sleep(Duration::from_secs(2));
        }

        bail!("timed out waiting for delivery artifact {target_name}")
    }

    pub fn download_delivery_envelope(
        &self,
        owner: &str,
        repo: &str,
        artifact_id: u64,
    ) -> Result<DeliveryEnvelope> {
        let response = self
            .http
            .get(format!(
                "{API_BASE}/repos/{owner}/{repo}/actions/artifacts/{artifact_id}/zip"
            ))
            .send()?;
        let bytes = response.bytes()?;
        let cursor = Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor)?;
        if archive.is_empty() {
            bail!("delivery artifact archive was empty");
        }
        let mut payload = archive.by_index(0)?;
        let mut buffer = String::new();
        payload.read_to_string(&mut buffer)?;
        serde_json::from_str(&buffer).context("failed to parse delivery envelope")
    }

    pub fn fetch_secret_via_delivery(
        &self,
        config: &AppConfig,
        session: &DeliverySession,
        project: &str,
        environment: &str,
        logical_key: &str,
        secret_name: &str,
    ) -> Result<String> {
        self.dispatch_delivery(
            config,
            session,
            project,
            environment,
            logical_key,
            secret_name,
        )?;

        let artifact = self.wait_for_delivery_artifact(
            &config.github_owner,
            &config.control_repo,
            session.request_id,
            session.ttl(),
        )?;
        let envelope = self.download_delivery_envelope(
            &config.github_owner,
            &config.control_repo,
            artifact.id,
        )?;
        if envelope.request_id != session.request_id {
            bail!("received mismatched delivery response");
        }
        let payload = session.decrypt_payload(&envelope.encrypted_payload)?;
        #[derive(Deserialize)]
        struct RevealedPayload {
            value: String,
        }
        let payload: RevealedPayload =
            serde_json::from_str(&payload).context("failed to parse decrypted payload")?;
        Ok(payload.value)
    }

    pub fn write_artifact_cache(
        &self,
        artifact: &Artifact,
        envelope: &DeliveryEnvelope,
    ) -> Result<()> {
        let path = AppConfig::artifacts_dir()?.join(format!("{}.json", artifact.id));
        fs::create_dir_all(AppConfig::artifacts_dir()?)?;
        fs::write(path, serde_json::to_vec_pretty(envelope)?)?;
        Ok(())
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        let response = self.http.get(url).send()?;
        if !response.status().is_success() {
            return Err(read_error(response));
        }
        response.json().context("failed to decode GitHub response")
    }
}

pub fn encrypt_for_github_secret(public_key_b64: &str, value: &str) -> Result<String> {
    let key_bytes = STANDARD
        .decode(public_key_b64)
        .context("failed to decode repository public key")?;
    let public_key: PublicKey = key_bytes
        .try_into()
        .map_err(|_| anyhow!("invalid GitHub public key length"))?;

    let mut ciphertext = vec![0_u8; value.len() + SEAL_OVERHEAD];
    crypto_box_seal(&mut ciphertext, value.as_bytes(), &public_key)
        .map_err(|_| anyhow!("failed to encrypt GitHub secret payload"))?;
    Ok(STANDARD.encode(ciphertext))
}

fn read_error(response: Response) -> anyhow::Error {
    let status = response.status();
    let body = response
        .text()
        .unwrap_or_else(|_| "<unavailable>".to_string());
    anyhow!("GitHub API request failed with {status}: {body}")
}

#[cfg(test)]
mod tests {
    use crate::session::{DeliverySession, encrypt_for_session};

    use super::encrypt_for_github_secret;

    #[test]
    fn github_secret_encryption_is_compatible_with_session_sealed_box() {
        let session = DeliverySession::new();
        let ciphertext =
            encrypt_for_github_secret(&session.recipient_public_key_b64(), "hello-world").unwrap();
        assert_eq!(session.decrypt_payload(&ciphertext).unwrap(), "hello-world");
    }

    #[test]
    fn encrypted_session_payload_can_be_cached() {
        let session = DeliverySession::new();
        let ciphertext =
            encrypt_for_session(&session.recipient_public_key_b64(), "{\"value\":\"x\"}").unwrap();
        assert!(ciphertext.len() > 20);
    }
}
