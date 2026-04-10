use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::naming::vault_secret_name;

pub const DEFAULT_SCHEMA_FILE: &str = ".envcraft.schema";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSchema {
    pub project: String,
    #[serde(default)]
    pub environments: BTreeSet<String>,
    #[serde(default)]
    pub vars: BTreeMap<String, VariableSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VariableSpec {
    #[serde(default)]
    pub vault_key: Option<String>,
    #[serde(rename = "type", default = "default_var_type")]
    pub kind: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub docs: Option<String>,
    #[serde(default)]
    pub generate: bool,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_var_type() -> String {
    "secret".to_string()
}

fn default_true() -> bool {
    true
}

impl ProjectSchema {
    pub fn new(project: impl Into<String>, environments: impl IntoIterator<Item = String>) -> Self {
        Self {
            project: project.into(),
            environments: environments.into_iter().collect(),
            vars: BTreeMap::new(),
        }
    }

    pub fn schema_path(root: &Path) -> PathBuf {
        root.join(DEFAULT_SCHEMA_FILE)
    }

    pub fn load_from(root: &Path) -> Result<Self> {
        let path = Self::schema_path(root);
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read schema at {}", path.display()))?;
        serde_yaml::from_str(&raw).context("failed to parse .envcraft.schema")
    }

    pub fn save_to(&self, root: &Path) -> Result<PathBuf> {
        let path = Self::schema_path(root);
        let raw = serde_yaml::to_string(self)?;
        fs::write(&path, raw)?;
        Ok(path)
    }

    pub fn ensure_environment(&mut self, environment: &str) {
        self.environments.insert(environment.to_string());
    }

    pub fn upsert_var(
        &mut self,
        logical_key: &str,
        environment: &str,
        kind: impl Into<String>,
        description: Option<String>,
        docs: Option<String>,
        generate: bool,
        required: bool,
    ) {
        self.ensure_environment(environment);
        let vault_key = vault_secret_name(&self.project, environment, logical_key);
        let kind = kind.into();

        self.vars
            .entry(logical_key.to_string())
            .and_modify(|spec| {
                spec.vault_key = Some(vault_key.clone());
                spec.kind = kind.clone();
                spec.generate = generate;
                spec.required = required;
                if description.is_some() {
                    spec.description = description.clone();
                }
                if docs.is_some() {
                    spec.docs = docs.clone();
                }
            })
            .or_insert(VariableSpec {
                vault_key: Some(vault_key),
                kind,
                description,
                docs,
                generate,
                required,
            });
    }

    pub fn keys(&self) -> impl Iterator<Item = (&String, &VariableSpec)> {
        self.vars.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectSchema;
    use tempfile::tempdir;

    #[test]
    fn roundtrips_schema() {
        let dir = tempdir().unwrap();
        let mut schema = ProjectSchema::new("nui-app", ["dev".to_string(), "prod".to_string()]);
        schema.upsert_var(
            "DB_PASSWORD",
            "prod",
            "secret",
            Some("database password".to_string()),
            None,
            true,
            true,
        );

        let path = schema.save_to(dir.path()).unwrap();
        assert!(path.exists());

        let loaded = ProjectSchema::load_from(dir.path()).unwrap();
        assert_eq!(loaded.project, "nui-app");
        assert!(loaded.environments.contains("dev"));
        assert!(loaded.environments.contains("prod"));
        assert_eq!(
            loaded.vars["DB_PASSWORD"].vault_key.as_deref(),
            Some("NUI_APP_PROD_DB_PASSWORD")
        );
    }
}
