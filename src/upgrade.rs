use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use sha2::{Digest, Sha256};
use tar::Archive;

const OWNER: &str = "JhonaCodes";
const REPO: &str = "env-craft";
const BIN_NAME: &str = "envcraft";
const CHECKSUMS_FILE: &str = "SHA256SUMS";

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

    let checksums_url = checksums_download_url(version);
    match download_archive(&checksums_url) {
        Ok(checksums_bytes) => {
            let checksums_text =
                String::from_utf8(checksums_bytes).context("SHA256SUMS file is not valid UTF-8")?;
            verify_checksum(&checksums_text, &target.asset_name(), &bytes)?;
        }
        Err(_) => {
            eprintln!(
                "warning: SHA256SUMS not available for this release, skipping integrity check"
            );
        }
    }

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

pub fn checksums_download_url(version: Option<&str>) -> String {
    match version {
        Some(version) => {
            format!(
                "https://github.com/{OWNER}/{REPO}/releases/download/{version}/{CHECKSUMS_FILE}"
            )
        }
        None => {
            format!("https://github.com/{OWNER}/{REPO}/releases/latest/download/{CHECKSUMS_FILE}")
        }
    }
}

/// Verify that the SHA-256 digest of `archive_bytes` matches the expected hash
/// listed in the `checksums_text` for the given `asset_name`.
pub fn verify_checksum(checksums_text: &str, asset_name: &str, archive_bytes: &[u8]) -> Result<()> {
    let expected = parse_checksum_for_asset(checksums_text, asset_name)?;
    let actual = sha256_hex(archive_bytes);
    if actual != expected {
        bail!(
            "checksum mismatch for {asset_name}: expected {expected}, got {actual}. \
             The downloaded archive may have been tampered with."
        );
    }
    Ok(())
}

/// Parse a SHA256SUMS file and return the hex digest for `asset_name`.
/// Expected format per line: `<hex_hash>  <filename>` (two spaces, standard sha256sum output).
pub fn parse_checksum_for_asset(checksums_text: &str, asset_name: &str) -> Result<String> {
    for line in checksums_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Standard sha256sum format: "<hash>  <filename>" (two-space separator)
        // Also accept single space for flexibility.
        if let Some((hash, name)) = line.split_once(char::is_whitespace) {
            let name = name.trim();
            if name == asset_name {
                return Ok(hash.to_lowercase());
            }
        }
    }
    bail!("no checksum found for {asset_name} in SHA256SUMS")
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn download_archive(url: &str) -> Result<Vec<u8>> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("envcraft-upgrade/0.1.10"),
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
    // rename(2) on POSIX is atomic and replaces the destination in a single
    // kernel operation, avoiding the TOCTOU window that remove+rename creates.
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
    use super::{
        UpgradeTarget, checksums_download_url, parse_checksum_for_asset, release_download_url,
        sha256_hex, verify_checksum,
    };

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

    #[test]
    fn builds_checksums_url_latest() {
        assert_eq!(
            checksums_download_url(None),
            "https://github.com/JhonaCodes/env-craft/releases/latest/download/SHA256SUMS"
        );
    }

    #[test]
    fn builds_checksums_url_versioned() {
        assert_eq!(
            checksums_download_url(Some("v0.2.0")),
            "https://github.com/JhonaCodes/env-craft/releases/download/v0.2.0/SHA256SUMS"
        );
    }

    #[test]
    fn parses_checksum_from_sha256sums_file() {
        let checksums = "\
abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890  envcraft-macos-aarch64.tar.gz
1111111111111111111111111111111111111111111111111111111111111111  envcraft-linux-x86_64.tar.gz
";
        let hash = parse_checksum_for_asset(checksums, "envcraft-linux-x86_64.tar.gz").unwrap();
        assert_eq!(
            hash,
            "1111111111111111111111111111111111111111111111111111111111111111"
        );
    }

    #[test]
    fn parse_checksum_returns_error_for_missing_asset() {
        let checksums = "abcd1234  other-file.tar.gz\n";
        let result = parse_checksum_for_asset(checksums, "envcraft-macos-aarch64.tar.gz");
        assert!(result.is_err());
    }

    #[test]
    fn sha256_hex_produces_correct_digest() {
        // SHA-256 of empty input is a well-known constant
        let hash = sha256_hex(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn verify_checksum_passes_on_match() {
        let data = b"hello world";
        let hash = sha256_hex(data);
        let checksums = format!("{hash}  my-archive.tar.gz\n");
        verify_checksum(&checksums, "my-archive.tar.gz", data).unwrap();
    }

    #[test]
    fn verify_checksum_fails_on_mismatch() {
        let checksums =
            "0000000000000000000000000000000000000000000000000000000000000000  my-archive.tar.gz\n";
        let result = verify_checksum(checksums, "my-archive.tar.gz", b"actual data");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("checksum mismatch"));
    }
}
