use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
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
    ui::ProgressSpinner,
};

const API_BASE: &str = "https://api.github.com";
const ACCEPT_HEADER: &str = "application/vnd.github+json";
const API_VERSION: &str = "2022-11-28";
const SEAL_OVERHEAD: usize = 48;

#[derive(Debug, Clone)]
pub struct GitHubClient {
    backend: GitHubBackend,
}

#[derive(Debug, Clone)]
enum GitHubBackend {
    Http(Client),
    GhCli,
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

#[derive(Debug, Clone, Deserialize)]
pub struct Repository {
    pub name: String,
    pub clone_url: String,
    pub default_branch: String,
    pub html_url: String,
    pub private: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthenticatedUser {
    pub login: String,
}

#[derive(Debug, Clone)]
pub struct EnsureRepoResult {
    pub repo: Repository,
    pub created: bool,
}

#[derive(Debug, Deserialize)]
struct RepoSecretsResponse {
    secrets: Vec<RepoSecretMetadata>,
}

#[derive(Debug, Deserialize)]
struct ArtifactListResponse {
    artifacts: Vec<Artifact>,
}

#[derive(Debug, Deserialize)]
struct GhHostEntry {
    oauth_token: Option<String>,
}

impl GitHubClient {
    pub fn from_token_source(token_env_var: &str) -> Result<Self> {
        if let Ok(token) = std::env::var(token_env_var) {
            if !token.trim().is_empty() {
                return Self::new(token.trim());
            }
        }

        if let Some(token) = read_github_cli_token()? {
            return Self::new(&token);
        }

        if let Ok(client) = Self::from_gh_cli_auth() {
            return Ok(client);
        }

        bail!(
            "missing GitHub token in {token_env_var} and unable to read local GitHub CLI auth. Run `gh auth login` or export {token_env_var}"
        );
    }

    pub fn from_config(config: &AppConfig) -> Result<Self> {
        Self::from_token_source(&config.token_env_var)
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
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&format!("envcraft/{}", env!("CARGO_PKG_VERSION")))?,
        );

        let http = Client::builder().default_headers(headers).build()?;
        Ok(Self {
            backend: GitHubBackend::Http(http),
        })
    }

    pub fn from_gh_cli_auth() -> Result<Self> {
        let output = Command::new("gh")
            .args(["auth", "status", "--hostname", "github.com"])
            .output()
            .context("failed to execute `gh auth status`")?;

        if !output.status.success() {
            bail!(
                "GitHub CLI is not authenticated for github.com. Run `gh auth login` or export GITHUB_TOKEN"
            );
        }

        Ok(Self {
            backend: GitHubBackend::GhCli,
        })
    }

    pub fn get_repo_public_key(&self, owner: &str, repo: &str) -> Result<RepoPublicKey> {
        self.get_json(&format!(
            "{API_BASE}/repos/{owner}/{repo}/actions/secrets/public-key"
        ))
    }

    pub fn current_user(&self) -> Result<AuthenticatedUser> {
        self.get_json(&format!("{API_BASE}/user"))
    }

    pub fn get_repo(&self, owner: &str, repo: &str) -> Result<Option<Repository>> {
        let url = format!("{API_BASE}/repos/{owner}/{repo}");

        match &self.backend {
            GitHubBackend::Http(http) => {
                let response = http.get(url).send()?;

                match response.status().as_u16() {
                    200 => Ok(Some(
                        response
                            .json()
                            .context("failed to decode repository payload")?,
                    )),
                    404 => Ok(None),
                    _ => Err(read_error(response)),
                }
            }
            GitHubBackend::GhCli => {
                let output = self.run_gh_api("GET", &url, None)?;
                if output.status.success() {
                    let repo: Repository = serde_json::from_slice(&output.stdout)
                        .context("failed to decode repository payload")?;
                    return Ok(Some(repo));
                }

                if gh_output_indicates_status(&output, 404) {
                    return Ok(None);
                }

                Err(gh_api_error("GET", &url, &output))
            }
        }
    }

    pub fn ensure_private_repo(&self, owner: &str, repo: &str) -> Result<EnsureRepoResult> {
        if let Some(repo_info) = self.get_repo(owner, repo)? {
            return Ok(EnsureRepoResult {
                repo: repo_info,
                created: false,
            });
        }

        let current_user = self.current_user()?;
        let endpoint = if current_user.login.eq_ignore_ascii_case(owner) {
            format!("{API_BASE}/user/repos")
        } else {
            format!("{API_BASE}/orgs/{owner}/repos")
        };

        let payload = json!({
            "name": repo,
            "private": true,
            "auto_init": false,
        });

        let repo_info: Repository = self.post_json(&endpoint, &payload)?;
        Ok(EnsureRepoResult {
            repo: repo_info,
            created: true,
        })
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

        self.put_json(
            &url,
            &json!({
                "encrypted_value": encrypted_value,
                "key_id": public_key.key_id,
            }),
        )
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

        self.post_json_empty(
            &url,
            &json!({
                "ref": config.default_ref,
                "inputs": {
                    "request_id": session.request_id.to_string(),
                    "project": project,
                    "environment": environment,
                    "logical_key": logical_key,
                    "secret_name": secret_name,
                    "recipient_public_key": session.recipient_public_key_b64(),
                }
            }),
        )
    }

    pub fn wait_for_delivery_artifact(
        &self,
        owner: &str,
        repo: &str,
        request_id: uuid::Uuid,
        timeout: Duration,
        spinner: &mut ProgressSpinner,
    ) -> Result<Artifact> {
        let started = Instant::now();
        let target_name = format!("envcraft-{request_id}");

        while started.elapsed() < timeout {
            spinner.tick();
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
        let bytes = self.get_bytes(&format!(
            "{API_BASE}/repos/{owner}/{repo}/actions/artifacts/{artifact_id}/zip"
        ))?;
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
        let mut spinner = ProgressSpinner::new(format!(
            "Waiting for GitHub Actions to deliver {environment}/{logical_key}"
        ));
        if let Err(error) = self.dispatch_delivery(
            config,
            session,
            project,
            environment,
            logical_key,
            secret_name,
        ) {
            spinner.fail(&format!("Failed to dispatch {environment}/{logical_key}"));
            return Err(error);
        }

        let artifact = match self.wait_for_delivery_artifact(
            &config.github_owner,
            &config.control_repo,
            session.request_id,
            session.ttl(),
            &mut spinner,
        ) {
            Ok(artifact) => artifact,
            Err(error) => {
                spinner.fail(&format!(
                    "Timed out waiting for {environment}/{logical_key}"
                ));
                return Err(error);
            }
        };
        let envelope = match self.download_delivery_envelope(
            &config.github_owner,
            &config.control_repo,
            artifact.id,
        ) {
            Ok(envelope) => envelope,
            Err(error) => {
                spinner.fail(&format!("Failed to download {environment}/{logical_key}"));
                return Err(error);
            }
        };
        if envelope.request_id != session.request_id {
            spinner.fail(&format!(
                "Received mismatched payload for {environment}/{logical_key}"
            ));
            bail!("received mismatched delivery response");
        }
        let payload = match session.decrypt_payload(&envelope.encrypted_payload) {
            Ok(payload) => payload,
            Err(error) => {
                spinner.fail(&format!("Failed to decrypt {environment}/{logical_key}"));
                return Err(error);
            }
        };
        #[derive(Deserialize)]
        struct RevealedPayload {
            value: String,
        }
        let payload: RevealedPayload =
            match serde_json::from_str(&payload).context("failed to parse decrypted payload") {
                Ok(payload) => payload,
                Err(error) => {
                    spinner.fail(&format!("Failed to decode {environment}/{logical_key}"));
                    return Err(error);
                }
            };
        spinner.success(&format!(
            "Delivered {environment}/{logical_key} from GitHub Actions"
        ));
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
        match &self.backend {
            GitHubBackend::Http(http) => {
                let response = http.get(url).send()?;
                if !response.status().is_success() {
                    return Err(read_error(response));
                }
                response.json().context("failed to decode GitHub response")
            }
            GitHubBackend::GhCli => {
                let output = self.run_gh_api("GET", url, None)?;
                if !output.status.success() {
                    return Err(gh_api_error("GET", url, &output));
                }
                serde_json::from_slice(&output.stdout).context("failed to decode GitHub response")
            }
        }
    }

    fn post_json<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<T> {
        match &self.backend {
            GitHubBackend::Http(http) => {
                let response = http.post(url).json(payload).send()?;
                if !response.status().is_success() {
                    return Err(read_error(response));
                }
                response.json().context("failed to decode GitHub response")
            }
            GitHubBackend::GhCli => {
                let output = self.run_gh_api("POST", url, Some(payload))?;
                if !output.status.success() {
                    return Err(gh_api_error("POST", url, &output));
                }
                serde_json::from_slice(&output.stdout).context("failed to decode GitHub response")
            }
        }
    }

    fn post_json_empty(&self, url: &str, payload: &serde_json::Value) -> Result<()> {
        match &self.backend {
            GitHubBackend::Http(http) => {
                let response = http.post(url).json(payload).send()?;
                if !response.status().is_success() {
                    return Err(read_error(response));
                }
                Ok(())
            }
            GitHubBackend::GhCli => {
                let output = self.run_gh_api("POST", url, Some(payload))?;
                if !output.status.success() {
                    return Err(gh_api_error("POST", url, &output));
                }
                Ok(())
            }
        }
    }

    fn put_json(&self, url: &str, payload: &serde_json::Value) -> Result<()> {
        match &self.backend {
            GitHubBackend::Http(http) => {
                let response = http.put(url).json(payload).send()?;
                if !response.status().is_success() {
                    return Err(read_error(response));
                }
                Ok(())
            }
            GitHubBackend::GhCli => {
                let output = self.run_gh_api("PUT", url, Some(payload))?;
                if !output.status.success() {
                    return Err(gh_api_error("PUT", url, &output));
                }
                Ok(())
            }
        }
    }

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        match &self.backend {
            GitHubBackend::Http(http) => {
                let response = http.get(url).send()?;
                if !response.status().is_success() {
                    return Err(read_error(response));
                }
                Ok(response.bytes()?.to_vec())
            }
            GitHubBackend::GhCli => {
                let output = self.run_gh_api("GET", url, None)?;
                if !output.status.success() {
                    return Err(gh_api_error("GET", url, &output));
                }
                Ok(output.stdout)
            }
        }
    }

    fn run_gh_api(
        &self,
        method: &str,
        url: &str,
        payload: Option<&serde_json::Value>,
    ) -> Result<Output> {
        let endpoint = gh_api_endpoint(url);
        let mut command = Command::new("gh");
        command
            .arg("api")
            .arg("--method")
            .arg(method)
            .arg("-H")
            .arg(format!("Accept: {ACCEPT_HEADER}"))
            .arg("-H")
            .arg(format!("X-GitHub-Api-Version: {API_VERSION}"));

        if payload.is_some() {
            command.arg("--input").arg("-");
        }

        command
            .arg(endpoint)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if payload.is_some() {
            command.stdin(Stdio::piped());
        }

        let mut child = command
            .spawn()
            .with_context(|| format!("failed to execute `gh api` for {method} {url}"))?;

        if let Some(payload) = payload {
            let body = serde_json::to_vec(payload).context("failed to encode GitHub payload")?;
            let mut stdin = child
                .stdin
                .take()
                .context("failed to open stdin for `gh api`")?;
            stdin
                .write_all(&body)
                .context("failed to send payload to `gh api`")?;
        }

        child
            .wait_with_output()
            .with_context(|| format!("failed to wait for `gh api` response on {method} {url}"))
    }
}

fn read_github_cli_token() -> Result<Option<String>> {
    for path in github_cli_hosts_candidates() {
        if !path.exists() {
            continue;
        }

        if let Some(token) = read_github_cli_token_from_hosts(&path)? {
            return Ok(Some(token));
        }
    }

    Ok(None)
}

fn read_github_cli_token_from_hosts(path: &Path) -> Result<Option<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read GitHub CLI hosts file at {}", path.display()))?;
    let hosts: BTreeMap<String, GhHostEntry> =
        serde_yaml::from_str(&content).with_context(|| {
            format!(
                "failed to parse GitHub CLI hosts file at {}",
                path.display()
            )
        })?;

    if let Some(token) = hosts
        .get("github.com")
        .and_then(|entry| entry.oauth_token.as_deref())
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        return Ok(Some(token.to_string()));
    }

    Ok(hosts
        .values()
        .filter_map(|entry| entry.oauth_token.as_deref())
        .map(str::trim)
        .find(|token| !token.is_empty())
        .map(ToOwned::to_owned))
}

fn github_cli_hosts_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(dir) = std::env::var("GH_CONFIG_DIR") {
        let path = PathBuf::from(dir).join("hosts.yml");
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }

    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        let path = PathBuf::from(dir).join("gh").join("hosts.yml");
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }

    if let Some(dir) = dirs::config_dir() {
        let path = dir.join("gh").join("hosts.yml");
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }

    if let Some(home) = dirs::home_dir() {
        let path = home.join(".config").join("gh").join("hosts.yml");
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }

    candidates
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

fn gh_api_endpoint(url: &str) -> String {
    url.strip_prefix(API_BASE).unwrap_or(url).to_string()
}

fn gh_output_indicates_status(output: &Output, status: u16) -> bool {
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr.contains(&format!("HTTP {status}")) || stderr.contains(&format!("({status})"))
}

fn gh_api_error(method: &str, url: &str, output: &Output) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let body = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "<unavailable>".to_string()
    };
    anyhow!("GitHub CLI request failed for {method} {url}: {body}")
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use tempfile::tempdir;

    use crate::session::{DeliverySession, encrypt_for_session};

    use super::{
        GitHubClient, encrypt_for_github_secret, gh_api_endpoint, github_cli_hosts_candidates,
        read_github_cli_token_from_hosts,
    };

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

    #[test]
    fn builds_client_with_direct_token() {
        let client = GitHubClient::new("fake-token");
        assert!(client.is_ok());
    }

    #[test]
    fn reads_github_cli_token_from_hosts_file() {
        let temp = tempdir().unwrap();
        let hosts = temp.path().join("hosts.yml");
        fs::write(
            &hosts,
            "github.com:\n  oauth_token: test-token\n  user: jhonacode\n",
        )
        .unwrap();

        let token = read_github_cli_token_from_hosts(&hosts).unwrap();
        assert_eq!(token.as_deref(), Some("test-token"));
    }

    #[test]
    fn gh_config_dir_is_preferred_candidate() {
        let temp = tempdir().unwrap();
        let original = env::var_os("GH_CONFIG_DIR");
        unsafe { env::set_var("GH_CONFIG_DIR", temp.path()) };

        let candidates = github_cli_hosts_candidates();

        match original {
            Some(value) => unsafe { env::set_var("GH_CONFIG_DIR", value) },
            None => unsafe { env::remove_var("GH_CONFIG_DIR") },
        }

        assert_eq!(candidates.first(), Some(&temp.path().join("hosts.yml")));
    }

    #[test]
    fn strips_api_base_for_gh_cli_endpoints() {
        assert_eq!(
            gh_api_endpoint("https://api.github.com/repos/JhonaCodes/env-craft"),
            "/repos/JhonaCodes/env-craft"
        );
    }
}
