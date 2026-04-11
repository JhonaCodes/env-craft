use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{config::AppConfig, fs_sec, github::GitHubClient};

const GITHUB_WEB_BASE: &str = "https://github.com";
const GITHUB_API_BASE: &str = "https://api.github.com";
const ACCEPT_HEADER: &str = "application/vnd.github+json";
const API_VERSION: &str = "2022-11-28";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(900);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredGitHubAppMetadata {
    pub app_id: String,
    pub slug: String,
    pub install_url: String,
    pub html_url: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub ci_repos: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GitHubAppSetupResult {
    pub app_id: String,
    pub slug: String,
    pub install_url: String,
    pub launcher_path: Option<PathBuf>,
    pub seeded_ci_repos: Vec<String>,
    pub created: bool,
}

#[derive(Debug, Clone)]
pub struct GitHubAppConnectResult {
    pub app_id: String,
    pub slug: String,
    pub install_url: String,
    pub seeded_ci_repos: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ManifestConversionResponse {
    id: u64,
    slug: String,
    pem: String,
    html_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct GitHubAppManifest {
    name: String,
    url: String,
    redirect_url: String,
    callback_urls: Vec<String>,
    description: String,
    public: bool,
    default_permissions: BTreeMap<String, String>,
}

pub fn setup_github_app(config: &AppConfig, open_browser: bool) -> Result<GitHubAppSetupResult> {
    config.ensure_local_dirs()?;

    if let Some(metadata) = load_stored_metadata(config)? {
        let github = GitHubClient::from_token_source(&config.token_env_var)?;
        if github.get_app_by_slug(&metadata.slug)?.is_some() {
            return Ok(GitHubAppSetupResult {
                app_id: metadata.app_id,
                slug: metadata.slug,
                install_url: metadata.install_url,
                launcher_path: None,
                seeded_ci_repos: Vec::new(),
                created: false,
            });
        }

        remove_stale_github_app(config)?;
    }

    let state = Uuid::new_v4().to_string();
    let callback_server = CallbackServer::bind()?;
    let redirect_url = callback_server.redirect_url();
    let manifest = build_manifest(config, &redirect_url);
    let launcher_path = write_registration_launcher(config, &manifest, &state)?;

    if open_browser {
        let _ = open_path_in_browser(&launcher_path);
    }

    let code = callback_server.wait_for_code(&state)?;
    let response = exchange_manifest_code(&code)?;
    let install_url = format!("{GITHUB_WEB_BASE}/apps/{}/installations/new", response.slug);

    let metadata = StoredGitHubAppMetadata {
        app_id: response.id.to_string(),
        slug: response.slug.clone(),
        install_url: install_url.clone(),
        html_url: response.html_url.clone(),
        created_at: Utc::now(),
        ci_repos: Vec::new(),
    };
    persist_github_app(config, &metadata, &response.pem)?;
    Ok(GitHubAppSetupResult {
        app_id: response.id.to_string(),
        slug: response.slug,
        install_url,
        launcher_path: Some(launcher_path),
        seeded_ci_repos: Vec::new(),
        created: true,
    })
}

pub fn connect_github_app(
    config: &AppConfig,
    ci_repos: &[String],
) -> Result<GitHubAppConnectResult> {
    let mut metadata = load_stored_metadata(config)?.ok_or_else(|| {
        anyhow!(
            "no GitHub App is configured locally for {}. Run `envcraft github-app setup` first",
            config.control_repo_slug()
        )
    })?;

    let seeded_ci_repos = connect_ci_repos(config, &mut metadata, ci_repos)?;
    Ok(GitHubAppConnectResult {
        app_id: metadata.app_id,
        slug: metadata.slug,
        install_url: metadata.install_url,
        seeded_ci_repos,
    })
}

pub fn load_stored_metadata(config: &AppConfig) -> Result<Option<StoredGitHubAppMetadata>> {
    let path = config.github_app_metadata_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read GitHub App metadata at {}", path.display()))?;
    let metadata = toml::from_str(&raw)
        .with_context(|| format!("failed to parse GitHub App metadata at {}", path.display()))?;
    Ok(Some(metadata))
}

fn build_manifest(config: &AppConfig, redirect_url: &str) -> GitHubAppManifest {
    let mut default_permissions = BTreeMap::new();
    default_permissions.insert("actions".to_string(), "write".to_string());
    default_permissions.insert("contents".to_string(), "write".to_string());
    default_permissions.insert("metadata".to_string(), "read".to_string());
    default_permissions.insert("secrets".to_string(), "write".to_string());
    default_permissions.insert("workflows".to_string(), "write".to_string());

    GitHubAppManifest {
        name: format!("envcraft-{}", config.control_repo),
        url: "https://github.com/JhonaCodes/env-craft".to_string(),
        redirect_url: redirect_url.to_string(),
        callback_urls: vec![redirect_url.to_string()],
        description: format!(
            "EnvCraft CI reader for the {} control-plane repository",
            config.control_repo_slug()
        ),
        public: false,
        default_permissions,
    }
}

fn write_registration_launcher(
    config: &AppConfig,
    manifest: &GitHubAppManifest,
    state: &str,
) -> Result<PathBuf> {
    let registration_url = registration_url(config, state)?;
    let manifest_json =
        serde_json::to_string(manifest).context("failed to encode GitHub App manifest")?;
    let html = format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>EnvCraft GitHub App setup</title>
  </head>
  <body>
    <p>Redirecting to GitHub to create the EnvCraft GitHub App...</p>
    <form id="register" action="{registration_url}" method="post">
      <input type="hidden" name="manifest" value='{manifest}' />
    </form>
    <script>
      document.getElementById("register").submit();
    </script>
  </body>
</html>
"#,
        registration_url = registration_url,
        manifest = html_attr_escape(&manifest_json)
    );

    let path = AppConfig::cache_dir()?.join("github-app-registration.html");
    fs::write(&path, html)
        .with_context(|| format!("failed to write launcher page at {}", path.display()))?;
    Ok(path)
}

fn registration_url(config: &AppConfig, state: &str) -> Result<String> {
    let owner = &config.github_owner;
    let endpoint = if owner_matches_authenticated_user(owner)? {
        format!("{GITHUB_WEB_BASE}/settings/apps/new?state={state}")
    } else {
        format!("{GITHUB_WEB_BASE}/organizations/{owner}/settings/apps/new?state={state}")
    };

    Ok(endpoint)
}

fn owner_matches_authenticated_user(owner: &str) -> Result<bool> {
    let client = match GitHubClient::from_token_source("GITHUB_TOKEN") {
        Ok(client) => client,
        Err(_) => return Ok(false),
    };
    let current_user = client.current_user()?;
    Ok(current_user.login.eq_ignore_ascii_case(owner))
}

fn persist_github_app(
    config: &AppConfig,
    metadata: &StoredGitHubAppMetadata,
    private_key_pem: &str,
) -> Result<()> {
    let key_path = config.github_app_private_key_path()?;
    let metadata_path = config.github_app_metadata_path()?;

    fs_sec::write_secret_file(&key_path, private_key_pem.as_bytes())?;
    fs_sec::write_secret_file(
        &metadata_path,
        toml::to_string_pretty(&metadata)?.as_bytes(),
    )?;
    Ok(())
}

fn remove_stale_github_app(config: &AppConfig) -> Result<()> {
    let key_path = config.github_app_private_key_path()?;
    let metadata_path = config.github_app_metadata_path()?;

    if metadata_path.exists() {
        fs::remove_file(&metadata_path).with_context(|| {
            format!(
                "failed to remove stale GitHub App metadata at {}",
                metadata_path.display()
            )
        })?;
    }

    if key_path.exists() {
        fs::remove_file(&key_path).with_context(|| {
            format!(
                "failed to remove stale GitHub App private key at {}",
                key_path.display()
            )
        })?;
    }

    Ok(())
}

fn connect_ci_repos(
    config: &AppConfig,
    metadata: &mut StoredGitHubAppMetadata,
    ci_repos: &[String],
) -> Result<Vec<String>> {
    let private_key_path = config.github_app_private_key_path()?;
    let private_key_pem = fs::read_to_string(&private_key_path).with_context(|| {
        format!(
            "failed to read stored GitHub App private key at {}",
            private_key_path.display()
        )
    })?;

    let seeded_ci_repos =
        seed_ci_repo_secrets(config, ci_repos, &metadata.app_id, &private_key_pem)?;
    if seeded_ci_repos.is_empty() {
        return Ok(seeded_ci_repos);
    }

    for repo in &seeded_ci_repos {
        if !metadata.ci_repos.iter().any(|existing| existing == repo) {
            metadata.ci_repos.push(repo.clone());
        }
    }
    metadata.ci_repos.sort();
    metadata.ci_repos.dedup();
    persist_github_app(config, metadata, &private_key_pem)?;
    Ok(seeded_ci_repos)
}

fn seed_ci_repo_secrets(
    config: &AppConfig,
    ci_repos: &[String],
    app_id: &str,
    private_key_pem: &str,
) -> Result<Vec<String>> {
    if ci_repos.is_empty() {
        return Ok(Vec::new());
    }

    let github = GitHubClient::from_token_source(&config.token_env_var)?;
    let mut seeded = Vec::new();

    for repo in ci_repos {
        let (owner, repo_name) = resolve_ci_repo_target(config, &github, repo)?;
        github.put_repo_secret(&owner, &repo_name, &config.github_app_id_env_var, app_id)?;
        github.put_repo_secret(
            &owner,
            &repo_name,
            &config.github_app_private_key_env_var,
            private_key_pem,
        )?;
        seeded.push(format!("{owner}/{repo_name}"));
    }

    Ok(seeded)
}

fn resolve_ci_repo_target(
    config: &AppConfig,
    github: &GitHubClient,
    input: &str,
) -> Result<(String, String)> {
    let trimmed = input.trim();
    let (owner, repo) = match trimmed.split_once('/') {
        Some((owner, repo)) => (owner.trim().to_string(), repo.trim().to_string()),
        None => (config.github_owner.clone(), trimmed.to_string()),
    };

    if github.get_repo(&owner, &repo)?.is_some() {
        return Ok((owner, repo));
    }

    if !trimmed.contains('/') && repo.contains('_') {
        let alternate = repo.replace('_', "-");
        if alternate != repo && github.get_repo(&owner, &alternate)?.is_some() {
            bail!(
                "repository `{owner}/{repo}` was not found. Did you mean `{owner}/{alternate}`? Re-run `envcraft github-app connect --ci-repo {alternate}`"
            );
        }
    }

    bail!(
        "repository `{owner}/{repo}` was not found. Pass the exact repository slug, for example `--ci-repo {owner}/your-repo`"
    );
}

fn exchange_manifest_code(code: &str) -> Result<ManifestConversionResponse> {
    let client = Client::builder().build()?;
    let url = format!("{GITHUB_API_BASE}/app-manifests/{code}/conversions");
    let response = client
        .post(&url)
        .header(ACCEPT, HeaderValue::from_static(ACCEPT_HEADER))
        .header(
            "X-GitHub-Api-Version",
            HeaderValue::from_static(API_VERSION),
        )
        .header(
            USER_AGENT,
            HeaderValue::from_str(&format!("envcraft/{}", env!("CARGO_PKG_VERSION")))?,
        )
        .json(&json!({}))
        .send()
        .context("failed to exchange GitHub App manifest code")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|_| "<unavailable>".to_string());
        bail!("GitHub App manifest conversion failed with {status}: {body}");
    }

    response
        .json()
        .context("failed to decode GitHub App manifest conversion response")
}

struct CallbackServer {
    listener: TcpListener,
}

impl CallbackServer {
    fn bind() -> Result<Self> {
        let listener =
            TcpListener::bind("127.0.0.1:0").context("failed to bind local callback server")?;
        listener
            .set_nonblocking(true)
            .context("failed to configure local callback server")?;
        Ok(Self { listener })
    }

    fn redirect_url(&self) -> String {
        let port = self
            .listener
            .local_addr()
            .map(|addr| addr.port())
            .unwrap_or(0);
        format!("http://127.0.0.1:{port}/callback")
    }

    fn wait_for_code(self, expected_state: &str) -> Result<String> {
        let started = Instant::now();
        loop {
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    let callback = read_callback_request(&mut stream)?;
                    write_callback_response(&mut stream)?;

                    if callback.state != expected_state {
                        bail!("received GitHub App callback with an unexpected state");
                    }

                    return Ok(callback.code);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if started.elapsed() > CALLBACK_TIMEOUT {
                        bail!(
                            "timed out waiting for the GitHub App callback. Re-run the command and finish the setup within 15 minutes"
                        );
                    }
                    thread::sleep(Duration::from_millis(250));
                }
                Err(error) => {
                    return Err(error).context("failed while waiting for GitHub App callback");
                }
            }
        }
    }
}

#[derive(Debug)]
struct CallbackPayload {
    code: String,
    state: String,
}

fn read_callback_request(stream: &mut TcpStream) -> Result<CallbackPayload> {
    let mut buffer = [0_u8; 8192];
    let size = stream
        .read(&mut buffer)
        .context("failed to read GitHub App callback request")?;
    let request = String::from_utf8_lossy(&buffer[..size]);
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| anyhow!("callback request was empty"))?;

    let path = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("callback request line did not contain a path"))?;
    let query = path
        .split_once('?')
        .map(|(_, query)| query)
        .ok_or_else(|| anyhow!("callback request did not include query parameters"))?;

    let params = parse_query(query);
    let code = params
        .get("code")
        .cloned()
        .ok_or_else(|| anyhow!("callback request did not include a GitHub manifest code"))?;
    let state = params
        .get("state")
        .cloned()
        .ok_or_else(|| anyhow!("callback request did not include state"))?;

    Ok(CallbackPayload { code, state })
}

fn write_callback_response(stream: &mut TcpStream) -> Result<()> {
    let body = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>EnvCraft GitHub App setup complete</title>
  </head>
  <body>
    <p>EnvCraft captured the GitHub App registration callback. You can close this tab and return to the terminal.</p>
  </body>
</html>
"#;
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .context("failed to write GitHub App callback response")?;
    Ok(())
}

fn parse_query(query: &str) -> BTreeMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (percent_decode(key), percent_decode(value)))
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    decoded.push(byte);
                    index += 3;
                    continue;
                }
                decoded.push(bytes[index]);
                index += 1;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).to_string()
}

fn html_attr_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&#39;")
}

fn open_path_in_browser(path: &PathBuf) -> Result<()> {
    #[cfg(target_os = "macos")]
    let candidates = ["open"];
    #[cfg(not(target_os = "macos"))]
    let candidates = ["xdg-open"];

    for candidate in candidates {
        let status = Command::new(candidate).arg(path).status();
        if let Ok(status) = status {
            if status.success() {
                return Ok(());
            }
        }
    }

    bail!("failed to open {}", path.display())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::config::AppConfig;

    use super::{
        StoredGitHubAppMetadata, build_manifest, html_attr_escape, load_stored_metadata,
        parse_query, percent_decode,
    };

    #[test]
    fn build_manifest_uses_redirect_url() {
        let config = AppConfig {
            github_owner: "JhonaCodes".to_string(),
            control_repo: "envcraft-secrets".to_string(),
            deliver_workflow: "deliver.yml".to_string(),
            default_ref: "main".to_string(),
            token_env_var: "GITHUB_TOKEN".to_string(),
            github_app_id_env_var: "ENVCRAFT_GITHUB_APP_ID".to_string(),
            github_app_private_key_env_var: "ENVCRAFT_GITHUB_APP_PRIVATE_KEY".to_string(),
            github_app_private_key_file_env_var: "ENVCRAFT_GITHUB_APP_PRIVATE_KEY_FILE".to_string(),
            control_repo_local_path: None,
        };

        let manifest = build_manifest(&config, "http://127.0.0.1:9999/callback");
        assert_eq!(manifest.redirect_url, "http://127.0.0.1:9999/callback");
        assert_eq!(
            manifest.callback_urls,
            vec!["http://127.0.0.1:9999/callback"]
        );
        assert_eq!(
            manifest
                .default_permissions
                .get("secrets")
                .map(String::as_str),
            Some("write")
        );
    }

    #[test]
    fn query_parser_decodes_percent_encoding() {
        let parsed = parse_query("code=a180&state=abc%20123");
        assert_eq!(parsed.get("code").map(String::as_str), Some("a180"));
        assert_eq!(parsed.get("state").map(String::as_str), Some("abc 123"));
        assert_eq!(percent_decode("hello+world"), "hello world");
    }

    #[test]
    fn escapes_single_quotes_for_html_attributes() {
        assert_eq!(html_attr_escape("{'a':1}"), "{&#39;a&#39;:1}");
    }

    #[test]
    fn loads_stored_metadata_from_disk() {
        let dir = tempdir().unwrap();
        let config = AppConfig {
            github_owner: "JhonaCodes".to_string(),
            control_repo: "envcraft-secrets".to_string(),
            deliver_workflow: "deliver.yml".to_string(),
            default_ref: "main".to_string(),
            token_env_var: "GITHUB_TOKEN".to_string(),
            github_app_id_env_var: "ENVCRAFT_GITHUB_APP_ID".to_string(),
            github_app_private_key_env_var: "ENVCRAFT_GITHUB_APP_PRIVATE_KEY".to_string(),
            github_app_private_key_file_env_var: "ENVCRAFT_GITHUB_APP_PRIVATE_KEY_FILE".to_string(),
            control_repo_local_path: Some(dir.path().join("repos/envcraft-secrets")),
        };

        let apps_dir = dir.path().join("github-apps");
        std::fs::create_dir_all(&apps_dir).unwrap();
        let metadata_path = apps_dir.join("JhonaCodes-envcraft-secrets.toml");
        let metadata = StoredGitHubAppMetadata {
            app_id: "12345".to_string(),
            slug: "envcraft-secrets".to_string(),
            install_url: "https://github.com/apps/envcraft/installations/new".to_string(),
            html_url: None,
            created_at: chrono::Utc::now(),
            ci_repos: vec!["JhonaCodes/my-app".to_string()],
        };
        std::fs::write(&metadata_path, toml::to_string_pretty(&metadata).unwrap()).unwrap();

        // These helpers use the real home-based path, so this test only validates TOML shape.
        let raw = std::fs::read_to_string(&metadata_path).unwrap();
        let parsed: StoredGitHubAppMetadata = toml::from_str(&raw).unwrap();
        assert_eq!(parsed.app_id, "12345");
        assert_eq!(parsed.ci_repos, vec!["JhonaCodes/my-app".to_string()]);
        let _ = config;
        let _ = load_stored_metadata;
    }
}
