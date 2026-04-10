use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};

use crate::{
    bootstrap::bootstrap_control_plane,
    config::AppConfig,
    github::GitHubClient,
    naming::vault_secret_name,
    schema::ProjectSchema,
    secrets::{StackPreset, generate_from_presets, generate_secret_like},
    session::DeliverySession,
};

#[derive(Debug, Parser)]
#[command(
    name = "envcraft",
    version,
    about = "GitHub-backed environment secret orchestration"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init(InitArgs),
    Link(LinkArgs),
    Set(SetArgs),
    Generate(GenerateArgs),
    List(ListArgs),
    Pull(DeliverArgs),
    Reveal(RevealArgs),
    DeployInject(DeliverArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    #[arg(long)]
    github_owner: String,
    #[arg(long)]
    control_repo: String,
    #[arg(long, default_value = "deliver.yml")]
    workflow: String,
    #[arg(long, default_value = "main")]
    git_ref: String,
    #[arg(long, default_value = "GITHUB_TOKEN")]
    token_env_var: String,
    #[arg(long)]
    bootstrap_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct LinkArgs {
    #[arg(long)]
    project: String,
    #[arg(long = "env", required = true)]
    environments: Vec<String>,
    #[arg(long, default_value = ".")]
    root: PathBuf,
}

#[derive(Debug, Args)]
struct SetArgs {
    logical_key: String,
    #[arg(long = "env")]
    environment: String,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long)]
    value: Option<String>,
    #[arg(long, default_value = "secret")]
    kind: String,
    #[arg(long)]
    description: Option<String>,
    #[arg(long)]
    docs: Option<String>,
    #[arg(long, default_value_t = false)]
    generate: bool,
    #[arg(long, default_value_t = true)]
    required: bool,
}

#[derive(Debug, Args)]
struct GenerateArgs {
    #[arg(long = "env")]
    environment: String,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long = "preset", required = true)]
    presets: Vec<StackPreset>,
    #[arg(long = "extra-key")]
    extra_keys: Vec<String>,
    #[arg(long, default_value_t = true)]
    write_remote: bool,
}

#[derive(Debug, Args)]
struct ListArgs {
    #[arg(long = "env")]
    environment: Option<String>,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, default_value_t = false)]
    remote: bool,
}

#[derive(Debug, Args)]
struct DeliverArgs {
    #[arg(long = "env")]
    environment: String,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct RevealArgs {
    logical_key: String,
    #[arg(long = "env")]
    environment: String,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long)]
    output: Option<PathBuf>,
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init(args) => init(args),
        Command::Link(args) => link(args),
        Command::Set(args) => set(args),
        Command::Generate(args) => generate(args),
        Command::List(args) => list(args),
        Command::Pull(args) => pull(args),
        Command::Reveal(args) => reveal(args),
        Command::DeployInject(args) => deploy_inject(args),
    }
}

fn init(args: InitArgs) -> Result<()> {
    let config = AppConfig {
        github_owner: args.github_owner,
        control_repo: args.control_repo,
        deliver_workflow: args.workflow,
        default_ref: args.git_ref,
        token_env_var: args.token_env_var,
        control_repo_local_path: args.bootstrap_dir.clone(),
    };
    config.ensure_local_dirs()?;
    let config_path = config.save()?;

    println!("Saved EnvCraft config to {}", config_path.display());

    if let Some(dir) = args.bootstrap_dir {
        let created = bootstrap_control_plane(&dir, &config)?;
        println!("Bootstrapped control plane at {}", dir.display());
        for file in created {
            println!("  wrote {}", file.display());
        }
    }

    Ok(())
}

fn link(args: LinkArgs) -> Result<()> {
    let root = args.root;
    let schema_path = ProjectSchema::schema_path(&root);
    let envs: BTreeSet<_> = args.environments.into_iter().collect();
    let schema = if schema_path.exists() {
        let mut schema = ProjectSchema::load_from(&root)?;
        schema.project = args.project;
        schema.environments.extend(envs);
        schema
    } else {
        ProjectSchema::new(args.project, envs)
    };

    let path = schema.save_to(&root)?;
    AppConfig::write_gitignore_entries(&root)?;
    println!("Linked project with schema at {}", path.display());
    Ok(())
}

fn set(args: SetArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let root = args.root;
    let mut schema = load_or_bail_schema(&root)?;
    let value = match (args.generate, args.value) {
        (true, None) => generate_secret_like(&args.logical_key),
        (_, Some(value)) => value,
        (false, None) => rpassword::prompt_password(format!(
            "Value for {} ({}): ",
            args.logical_key, args.environment
        ))?,
    };

    schema.upsert_var(
        &args.logical_key,
        &args.environment,
        args.kind.clone(),
        args.description.clone(),
        args.docs.clone(),
        args.generate,
        args.required,
    );

    let secret_name = vault_secret_name(&schema.project, &args.environment, &args.logical_key);
    let github = GitHubClient::from_config(&config)?;
    github.put_repo_secret(
        &config.github_owner,
        &config.control_repo,
        &secret_name,
        &value,
    )?;
    let path = schema.save_to(&root)?;
    println!("Stored {}", secret_name);
    println!("Schema updated at {}", path.display());
    Ok(())
}

fn generate(args: GenerateArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let github = GitHubClient::from_config(&config)?;
    let root = args.root;
    let mut schema = load_or_bail_schema(&root)?;
    let mut vars = generate_from_presets(&args.presets);

    for key in args.extra_keys {
        vars.entry(key.clone())
            .or_insert_with(|| generate_secret_like(&key));
    }

    if vars.is_empty() {
        bail!("no variables requested");
    }

    for (logical_key, value) in &vars {
        schema.upsert_var(
            logical_key,
            &args.environment,
            "secret",
            Some("generated by envcraft".to_string()),
            None,
            true,
            true,
        );

        if args.write_remote {
            let secret_name = vault_secret_name(&schema.project, &args.environment, logical_key);
            github.put_repo_secret(
                &config.github_owner,
                &config.control_repo,
                &secret_name,
                value,
            )?;
        }
    }

    let path = schema.save_to(&root)?;
    println!(
        "Generated {} secrets for {}:{}",
        vars.len(),
        schema.project,
        args.environment
    );
    println!("Schema updated at {}", path.display());
    Ok(())
}

fn list(args: ListArgs) -> Result<()> {
    let schema = load_or_bail_schema(&args.root)?;
    let remote_metadata = if args.remote {
        let config = AppConfig::load()?;
        let github = GitHubClient::from_config(&config)?;
        let metadata = github.list_repo_secrets(&config.github_owner, &config.control_repo)?;
        Some(
            metadata
                .into_iter()
                .map(|item| (item.name.clone(), item))
                .collect::<BTreeMap<_, _>>(),
        )
    } else {
        None
    };

    println!("project: {}", schema.project);
    println!(
        "environments: {}",
        schema
            .environments
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );

    for (logical_key, spec) in schema.keys() {
        let secret_name = match &args.environment {
            Some(environment) => vault_secret_name(&schema.project, environment, logical_key),
            None => spec
                .vault_key
                .clone()
                .unwrap_or_else(|| logical_key.to_string()),
        };
        let remote_status = remote_metadata
            .as_ref()
            .map(|metadata| metadata.contains_key(&secret_name))
            .unwrap_or(false);
        println!(
            "- {:<24} {:<36} type={} generate={} required={} remote={}",
            logical_key, secret_name, spec.kind, spec.generate, spec.required, remote_status
        );
    }

    Ok(())
}

fn pull(args: DeliverArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let github = GitHubClient::from_config(&config)?;
    let schema = load_or_bail_schema(&args.root)?;
    let mut env_map = BTreeMap::new();

    for (logical_key, _) in schema.keys() {
        let session = DeliverySession::new();
        session.save()?;
        let secret_name = vault_secret_name(&schema.project, &args.environment, logical_key);
        let value = github.fetch_secret_via_delivery(
            &config,
            &session,
            &schema.project,
            &args.environment,
            logical_key,
            &secret_name,
        )?;
        env_map.insert(logical_key.clone(), value);
    }

    let output = args
        .output
        .unwrap_or_else(|| args.root.join(format!(".env.{}", args.environment)));
    write_dotenv(&output, &env_map)?;
    println!("Pulled {} secrets into {}", env_map.len(), output.display());
    Ok(())
}

fn reveal(args: RevealArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let github = GitHubClient::from_config(&config)?;
    let schema = load_or_bail_schema(&args.root)?;

    if !schema.vars.contains_key(&args.logical_key) {
        bail!("{} is not declared in .envcraft.schema", args.logical_key);
    }

    let session = DeliverySession::new();
    session.save()?;
    let secret_name = vault_secret_name(&schema.project, &args.environment, &args.logical_key);
    let value = github.fetch_secret_via_delivery(
        &config,
        &session,
        &schema.project,
        &args.environment,
        &args.logical_key,
        &secret_name,
    )?;

    if let Some(path) = args.output {
        fs::write(&path, format!("{}={}\n", args.logical_key, value))?;
        println!("Wrote reveal output to {}", path.display());
    } else {
        println!("{value}");
    }

    Ok(())
}

fn deploy_inject(args: DeliverArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let github = GitHubClient::from_config(&config)?;
    let schema = load_or_bail_schema(&args.root)?;
    let mut env_map = BTreeMap::new();

    for (logical_key, _) in schema.keys() {
        let session = DeliverySession::new();
        session.save()?;
        let secret_name = vault_secret_name(&schema.project, &args.environment, logical_key);
        let value = github.fetch_secret_via_delivery(
            &config,
            &session,
            &schema.project,
            &args.environment,
            logical_key,
            &secret_name,
        )?;
        env_map.insert(logical_key.clone(), value);
    }

    let shell_output = env_map
        .iter()
        .map(|(key, value)| format!("export {}='{}'", key, value.replace('\'', "'\"'\"'")))
        .collect::<Vec<_>>()
        .join("\n");

    if let Some(path) = args.output {
        fs::write(&path, format!("{shell_output}\n"))?;
        println!("Wrote deploy injection script to {}", path.display());
    } else {
        println!("{shell_output}");
    }

    Ok(())
}

fn load_or_bail_schema(root: &Path) -> Result<ProjectSchema> {
    ProjectSchema::load_from(root).with_context(|| {
        format!(
            "missing or invalid {} in {}",
            crate::schema::DEFAULT_SCHEMA_FILE,
            root.display()
        )
    })
}

fn write_dotenv(path: &Path, values: &BTreeMap<String, String>) -> Result<()> {
    let body = values
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, format!("{body}\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::{config::AppConfig, schema::ProjectSchema};

    use super::write_dotenv;

    #[test]
    fn writes_dotenv_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(".env.dev");
        let values = std::collections::BTreeMap::from([
            ("A".to_string(), "1".to_string()),
            ("B".to_string(), "2".to_string()),
        ]);

        write_dotenv(&path, &values).unwrap();
        let raw = std::fs::read_to_string(path).unwrap();
        assert!(raw.contains("A=1"));
        assert!(raw.contains("B=2"));
    }

    #[test]
    fn schema_creation_is_stable() {
        let dir = tempdir().unwrap();
        let schema = ProjectSchema::new("envcraft", ["dev".to_string(), "prod".to_string()]);
        schema.save_to(dir.path()).unwrap();
        assert!(dir.path().join(".envcraft.schema").exists());
    }

    #[test]
    fn app_config_gitignore_entries_can_be_written() {
        let dir = tempdir().unwrap();
        AppConfig::write_gitignore_entries(dir.path()).unwrap();
        let raw = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(raw.contains(".env"));
        assert!(raw.contains("!.envcraft.schema"));
    }
}
