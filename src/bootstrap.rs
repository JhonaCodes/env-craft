use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;

use crate::config::AppConfig;

const DELIVER_WORKFLOW: &str = r#"name: EnvCraft Deliver
run-name: envcraft-${{ inputs.request_id }}

on:
  workflow_dispatch:
    inputs:
      request_id:
        description: Unique EnvCraft request id
        required: true
        type: string
      project:
        description: Project slug
        required: true
        type: string
      environment:
        description: Environment slug
        required: true
        type: string
      logical_key:
        description: Logical key to reveal
        required: true
        type: string
      secret_name:
        description: Fully-qualified GitHub secret name
        required: true
        type: string
      recipient_public_key:
        description: Base64 recipient public key for sealed-box response
        required: true
        type: string

jobs:
  deliver:
    runs-on: ubuntu-latest
    concurrency:
      group: envcraft-deliver-${{ inputs.request_id }}
      cancel-in-progress: false
    env:
      REQUEST_ID: ${{ inputs.request_id }}
      PROJECT: ${{ inputs.project }}
      ENVIRONMENT: ${{ inputs.environment }}
      LOGICAL_KEY: ${{ inputs.logical_key }}
      SECRET_NAME: ${{ inputs.secret_name }}
      RECIPIENT_PUBLIC_KEY: ${{ inputs.recipient_public_key }}
      SECRET_VALUE: ${{ secrets[inputs.secret_name] }}
    steps:
      - name: Ensure secret exists
        if: ${{ env.SECRET_VALUE == '' }}
        run: |
          echo "Secret ${SECRET_NAME} is not available to this workflow." >&2
          exit 1

      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2

      - uses: actions/setup-node@49933ea5288caeca8642195f882d025a3c667d10 # v4.4.0
        with:
          node-version: '22'

      - name: Install libsodium
        run: npm install libsodium-wrappers

      - name: Encrypt payload
        run: node .github/scripts/envcraft-deliver.mjs

      - name: Upload encrypted payload
        uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4.6.2
        with:
          name: envcraft-${{ inputs.request_id }}
          path: payload.json
          retention-days: 1
"#;

const DELIVER_SCRIPT: &str = r#"import fs from "node:fs/promises";
import sodium from "libsodium-wrappers";

await sodium.ready;

const publicKey = sodium.from_base64(process.env.RECIPIENT_PUBLIC_KEY, sodium.base64_variants.ORIGINAL);
const plaintext = JSON.stringify({
  request_id: process.env.REQUEST_ID,
  project: process.env.PROJECT,
  environment: process.env.ENVIRONMENT,
  logical_key: process.env.LOGICAL_KEY,
  secret_name: process.env.SECRET_NAME,
  value: process.env.SECRET_VALUE,
  delivered_at: new Date().toISOString()
});

const ciphertext = sodium.crypto_box_seal(plaintext, publicKey);
const envelope = {
  request_id: process.env.REQUEST_ID,
  project: process.env.PROJECT,
  environment: process.env.ENVIRONMENT,
  logical_key: process.env.LOGICAL_KEY,
  secret_name: process.env.SECRET_NAME,
  encrypted_payload: sodium.to_base64(ciphertext, sodium.base64_variants.ORIGINAL),
  delivered_at: new Date().toISOString()
};

await fs.writeFile("payload.json", JSON.stringify(envelope, null, 2));
"#;

const CONTROL_PLANE_README: &str = r#"# EnvCraft Control Plane

This repository is managed by EnvCraft.

Responsibilities:
- host GitHub Actions workflows that can read GitHub Secrets
- store `.envcraft.schema` files under `projects/<project>/`
- expose encrypted delivery artifacts for `reveal`, `pull`, and `deploy-inject`

Expected layout:
- `.github/workflows/deliver.yml`
- `.github/scripts/envcraft-deliver.mjs`
- `projects/<project>/.envcraft.schema`

The code repository for EnvCraft is separate from this control-plane repository.
"#;

pub fn bootstrap_control_plane(root: &Path, config: &AppConfig) -> Result<Vec<PathBuf>> {
    let workflow_dir = root.join(".github/workflows");
    let script_dir = root.join(".github/scripts");
    let projects_dir = root.join("projects");

    fs::create_dir_all(&workflow_dir)?;
    fs::create_dir_all(&script_dir)?;
    fs::create_dir_all(&projects_dir)?;

    let workflow_path = workflow_dir.join(&config.deliver_workflow);
    let script_path = script_dir.join("envcraft-deliver.mjs");
    let readme_path = root.join("README.md");
    let gitkeep_path = projects_dir.join(".gitkeep");

    fs::write(&workflow_path, DELIVER_WORKFLOW)?;
    fs::write(&script_path, DELIVER_SCRIPT)?;
    if !readme_path.exists() {
        fs::write(&readme_path, CONTROL_PLANE_README)?;
    }
    if !gitkeep_path.exists() {
        fs::write(&gitkeep_path, "")?;
    }

    Ok(vec![workflow_path, script_path, readme_path, gitkeep_path])
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::config::AppConfig;

    use super::bootstrap_control_plane;

    #[test]
    fn writes_expected_control_plane_files() {
        let dir = tempdir().unwrap();
        let config = AppConfig {
            github_owner: "JhonaCodes".to_string(),
            control_repo: "envcraft-secrets".to_string(),
            deliver_workflow: "deliver.yml".to_string(),
            default_ref: "main".to_string(),
            token_env_var: "GITHUB_TOKEN".to_string(),
            control_repo_local_path: None,
        };

        let files = bootstrap_control_plane(dir.path(), &config).unwrap();
        assert!(files.iter().any(|path| path.ends_with("deliver.yml")));
        assert!(
            dir.path()
                .join(".github/scripts/envcraft-deliver.mjs")
                .exists()
        );
        let workflow =
            std::fs::read_to_string(dir.path().join(".github/workflows/deliver.yml")).unwrap();
        // Verify actions are pinned to full SHA, not mutable tags
        assert!(workflow.contains("actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683"));
        assert!(workflow.contains("actions/setup-node@49933ea5288caeca8642195f882d025a3c667d10"));
        assert!(
            workflow.contains("actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02")
        );
        // Verify comment tags are present for readability
        assert!(workflow.contains("# v4.2.2"));
        assert!(workflow.contains("# v4.4.0"));
        assert!(workflow.contains("# v4.6.2"));
    }

    #[test]
    fn workflow_does_not_contain_mutable_tags() {
        let dir = tempdir().unwrap();
        let config = AppConfig {
            github_owner: "JhonaCodes".to_string(),
            control_repo: "envcraft-secrets".to_string(),
            deliver_workflow: "deliver.yml".to_string(),
            default_ref: "main".to_string(),
            token_env_var: "GITHUB_TOKEN".to_string(),
            control_repo_local_path: None,
        };

        bootstrap_control_plane(dir.path(), &config).unwrap();
        let workflow =
            std::fs::read_to_string(dir.path().join(".github/workflows/deliver.yml")).unwrap();

        // No uses: line should end with a mutable @v* tag (only SHA pinning allowed)
        for line in workflow.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("- uses:") || trimmed.starts_with("uses:") {
                assert!(
                    !trimmed.ends_with("@v4") && !trimmed.ends_with("@v3"),
                    "found mutable tag in: {trimmed}"
                );
            }
        }
    }
}
