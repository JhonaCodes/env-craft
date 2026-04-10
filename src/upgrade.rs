use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use tar::Archive;

const OWNER: &str = "JhonaCodes";
const REPO: &str = "env-craft";
const BIN_NAME: &str = "envcraft";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeTarget {
    pub platform: String,
    pub arch: String,
}

impl UpgradeTarget {
    pub fn detect() -> Result<Self> {
        let platform = match std::env::consts::OS {
            "linux" => "linux",
            "macos" => "macos",
            other => bail!("unsupported OS for upgrade: {other}"),
        };

        let arch = match std::env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            other => bail!("unsupported architecture for upgrade: {other}"),
        };

        Ok(Self {
            platform: platform.to_string(),
            arch: arch.to_string(),
        })
    }

    pub fn asset_name(&self) -> String {
        format!("{BIN_NAME}-{}-{}.tar.gz", self.platform, self.arch)
    }
}

pub fn upgrade_binary(version: Option<&str>) -> Result<PathBuf> {
    let target = UpgradeTarget::detect()?;
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    let parent = current_exe
        .parent()
        .context("failed to resolve parent directory for current executable")?;
    let temp_output = parent.join(format!("{BIN_NAME}.upgrade.tmp"));

    let url = release_download_url(version, &target);
    let bytes = download_archive(&url)?;
    write_extracted_binary(&bytes, &temp_output)?;
    finalize_upgrade(&temp_output, &current_exe)?;

    Ok(current_exe)
}

pub fn release_download_url(version: Option<&str>, target: &UpgradeTarget) -> String {
    let asset = target.asset_name();
    match version {
        Some(version) => {
            format!("https://github.com/{OWNER}/{REPO}/releases/download/{version}/{asset}")
        }
        None => format!("https://github.com/{OWNER}/{REPO}/releases/latest/download/{asset}"),
    }
}

fn download_archive(url: &str) -> Result<Vec<u8>> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("envcraft-upgrade/0.1.1"),
    );
    let client = Client::builder().default_headers(headers).build()?;

    let response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to download upgrade archive from {url}"))?;

    if !response.status().is_success() {
        bail!(
            "failed to download upgrade archive from {url}: HTTP {}",
            response.status()
        );
    }

    Ok(response.bytes()?.to_vec())
}

fn write_extracted_binary(archive_bytes: &[u8], output_path: &Path) -> Result<()> {
    let decoder = GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = Archive::new(decoder);
    let mut found = false;

    for entry in archive
        .entries()
        .context("failed to inspect release archive")?
    {
        let mut entry = entry.context("failed to read entry from release archive")?;
        let path = entry
            .path()
            .context("failed to resolve archive entry path")?;
        if path == Path::new(BIN_NAME) {
            let mut buffer = Vec::new();
            entry
                .read_to_end(&mut buffer)
                .context("failed to read binary from release archive")?;
            fs::write(output_path, buffer)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let permissions = fs::Permissions::from_mode(0o755);
                fs::set_permissions(output_path, permissions)?;
            }
            found = true;
            break;
        }
    }

    if !found {
        bail!("release archive did not contain `{BIN_NAME}`");
    }

    Ok(())
}

fn finalize_upgrade(temp_output: &Path, current_exe: &Path) -> Result<()> {
    if current_exe.exists() {
        fs::remove_file(current_exe)
            .with_context(|| format!("failed to replace {}", current_exe.display()))?;
    }
    fs::rename(temp_output, current_exe).with_context(|| {
        format!(
            "failed to move upgraded binary into {}",
            current_exe.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{UpgradeTarget, release_download_url};

    #[test]
    fn builds_latest_download_url() {
        let target = UpgradeTarget {
            platform: "macos".to_string(),
            arch: "aarch64".to_string(),
        };

        assert_eq!(
            release_download_url(None, &target),
            "https://github.com/JhonaCodes/env-craft/releases/latest/download/envcraft-macos-aarch64.tar.gz"
        );
    }

    #[test]
    fn builds_versioned_download_url() {
        let target = UpgradeTarget {
            platform: "linux".to_string(),
            arch: "x86_64".to_string(),
        };

        assert_eq!(
            release_download_url(Some("v0.1.1"), &target),
            "https://github.com/JhonaCodes/env-craft/releases/download/v0.1.1/envcraft-linux-x86_64.tar.gz"
        );
    }
}
