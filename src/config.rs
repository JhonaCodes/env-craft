use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub github_owner: String,
    pub control_repo: String,
    #[serde(default = "default_workflow_file")]
    pub deliver_workflow: String,
    #[serde(default = "default_branch")]
    pub default_ref: String,
    #[serde(default = "default_token_env_var")]
    pub token_env_var: String,
    #[serde(default)]
    pub control_repo_local_path: Option<PathBuf>,
}

fn default_workflow_file() -> String {
    "deliver.yml".to_string()
}

fn default_branch() -> String {
    "main".to_string()
}

fn default_token_env_var() -> String {
    "GITHUB_TOKEN".to_string()
}

impl AppConfig {
    pub fn control_repo_slug(&self) -> String {
        format!("{}/{}", self.github_owner, self.control_repo)
    }

    pub fn config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("unable to resolve home directory"))?;
        Ok(home.join(".envcraft"))
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn cache_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("cache"))
    }

    pub fn requests_dir() -> Result<PathBuf> {
        Ok(Self::cache_dir()?.join("requests"))
    }

    pub fn artifacts_dir() -> Result<PathBuf> {
        Ok(Self::cache_dir()?.join("artifacts"))
    }

    pub fn control_repos_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("repos"))
    }

    pub fn default_control_repo_path(&self) -> Result<PathBuf> {
        Ok(Self::control_repos_dir()?.join(&self.control_repo))
    }

    pub fn control_repo_path(&self) -> Result<PathBuf> {
        match &self.control_repo_local_path {
            Some(path) => Ok(path.clone()),
            None => self.default_control_repo_path(),
        }
    }

    pub fn ensure_local_dirs(&self) -> Result<()> {
        fs::create_dir_all(Self::requests_dir()?)?;
        fs::create_dir_all(Self::artifacts_dir()?)?;
        fs::create_dir_all(Self::control_repos_dir()?)?;
        Ok(())
    }

    pub fn save(&self) -> Result<PathBuf> {
        let dir = Self::config_dir()?;
        fs::create_dir_all(&dir)?;
        let path = Self::config_path()?;
        let body = toml::to_string_pretty(self)?;
        fs::write(&path, body)?;
        Ok(path)
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config at {}", path.display()))?;
        toml::from_str(&raw).context("failed to parse EnvCraft config")
    }

    pub fn load_optional() -> Result<Option<Self>> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(Self::load()?))
    }

    pub fn write_gitignore_entries(repo_root: &Path) -> Result<()> {
        let gitignore_path = repo_root.join(".gitignore");
        let mut existing = if gitignore_path.exists() {
            fs::read_to_string(&gitignore_path)?
        } else {
            String::new()
        };

        let required = [
            ".env",
            ".env.*",
            "!.envcraft.schema",
            ".envcraft/",
            ".envcraft.generated.*",
        ];

        let mut changed = false;
        for entry in required {
            if !existing.lines().any(|line| line.trim() == entry) {
                if !existing.ends_with('\n') && !existing.is_empty() {
                    existing.push('\n');
                }
                existing.push_str(entry);
                existing.push('\n');
                changed = true;
            }
        }

        if changed {
            fs::write(&gitignore_path, existing)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;

    #[test]
    fn builds_control_repo_slug() {
        let config = AppConfig {
            github_owner: "JhonaCodes".to_string(),
            control_repo: "envcraft-secrets".to_string(),
            deliver_workflow: "deliver.yml".to_string(),
            default_ref: "main".to_string(),
            token_env_var: "GITHUB_TOKEN".to_string(),
            control_repo_local_path: None,
        };

        assert_eq!(config.control_repo_slug(), "JhonaCodes/envcraft-secrets");
    }

    #[test]
    fn builds_default_control_repo_path() {
        let config = AppConfig {
            github_owner: "JhonaCodes".to_string(),
            control_repo: "envcraft-secrets".to_string(),
            deliver_workflow: "deliver.yml".to_string(),
            default_ref: "main".to_string(),
            token_env_var: "GITHUB_TOKEN".to_string(),
            control_repo_local_path: None,
        };

        let path = config.default_control_repo_path().unwrap();
        assert!(path.ends_with(".envcraft/repos/envcraft-secrets"));
    }
}
