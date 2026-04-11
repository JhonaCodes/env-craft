use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};

use crate::{
    bootstrap::bootstrap_control_plane,
    config::AppConfig,
    github::GitHubClient,
    schema::ProjectSchema,
    secrets::{StackPreset, generate_from_presets, generate_secret_like},
    session::DeliverySession,
    upgrade::upgrade_binary,
};

#[derive(Debug, Parser)]
#[command(
    name = "envcraft",
    version,
    about = "Manage project secrets through GitHub Secrets and GitHub Actions",
    long_about = "EnvCraft is a global CLI for developers who manage many repositories. \
It keeps secret values in GitHub Secrets, uses GitHub Actions as the only authorized reader, \
and resolves the active project from the current directory's .envcraft.schema by default."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Create or update the global EnvCraft configuration")]
    Init(InitArgs),
    #[command(about = "Link the current repository to an EnvCraft project")]
    Link(LinkArgs),
    #[command(about = "Create or update a single secret in the control-plane repository")]
    Set(SetArgs),
    #[command(about = "Generate a group of standard secrets from stack presets")]
    Generate(GenerateArgs),
    #[command(about = "List logical variables and, optionally, remote secret availability")]
    List(ListArgs),
    #[command(about = "Download the latest EnvCraft release and replace the current binary")]
    Upgrade(UpgradeArgs),
    #[command(about = "Resolve every declared key for one environment into a local .env file")]
    Pull(DeliverArgs),
    #[command(about = "Reveal one logical key through a one-time GitHub Actions delivery flow")]
    Reveal(RevealArgs),
    #[command(
        about = "Emit shell exports for deploy-time injection without baking secrets into images"
    )]
    DeployInject(DeliverArgs),
}

#[derive(Debug, Args)]
#[command(
    after_help = "Example:\n  envcraft init --github-owner JhonaCodes --control-repo envcraft-secrets --bootstrap-dir ~/code/envcraft-secrets"
)]
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
#[command(after_help = "Example:\n  envcraft link --project nui-app --env dev --env prod")]
struct LinkArgs {
    #[arg(long)]
    project: String,
    #[arg(long = "env", required = true)]
    environments: Vec<String>,
    #[arg(long, default_value = ".")]
    root: PathBuf,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  envcraft set DB_PASSWORD --env prod --generate\n  envcraft set STRIPE_SECRET_KEY --env prod --project billing-api --root ~/code/billing-api"
)]
struct SetArgs {
    logical_key: String,
    #[arg(long)]
    project: Option<String>,
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
#[command(
    after_help = "Example:\n  envcraft generate --env dev --preset postgres --preset jwt --extra-key INTERNAL_API_TOKEN"
)]
struct GenerateArgs {
    #[arg(long)]
    project: Option<String>,
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
#[command(after_help = "Examples:\n  envcraft list\n  envcraft list --remote --env prod")]
struct ListArgs {
    #[arg(long)]
    project: Option<String>,
    #[arg(long = "env")]
    environment: Option<String>,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, default_value_t = false)]
    remote: bool,
}

#[derive(Debug, Args)]
#[command(after_help = "Examples:\n  envcraft upgrade\n  envcraft upgrade --version v0.1.1")]
struct UpgradeArgs {
    #[arg(long)]
    version: Option<String>,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  envcraft pull --env dev --output .env.dev\n  envcraft deploy-inject --env prod > env.sh"
)]
struct DeliverArgs {
    #[arg(long)]
    project: Option<String>,
    #[arg(long = "env")]
    environment: String,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Examples:\n  envcraft reveal DB_PASSWORD --env prod\n  envcraft reveal JWT_SECRET --env prod --output /tmp/jwt.env"
)]
struct RevealArgs {
    logical_key: String,
    #[arg(long)]
    project: Option<String>,
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
        Command::Upgrade(args) => upgrade(args),
        Command::Pull(args) => pull(args),
        Command::Reveal(args) => reveal(args),
        Command::DeployInject(args) => deploy_inject(args),
    }
}

fn init(args: InitArgs) -> Result<()> {
    let mut config = AppConfig {
        github_owner: args.github_owner,
        control_repo: args.control_repo,
        deliver_workflow: args.workflow,
        default_ref: args.git_ref,
        token_env_var: args.token_env_var,
        control_repo_local_path: None,
    };
    let bootstrap_dir = args
        .bootstrap_dir
        .unwrap_or(config.default_control_repo_path()?);
    config.control_repo_local_path = Some(bootstrap_dir.clone());
    config.ensure_local_dirs()?;
    let github = GitHubClient::from_token_source(&config.token_env_var)?;
    let ensured = github.ensure_private_repo(&config.github_owner, &config.control_repo)?;
    config.default_ref = ensured.repo.default_branch.clone();
    ensure_local_control_repo(
        &bootstrap_dir,
        &ensured.repo.clone_url,
        &ensured.repo.default_branch,
    )?;
    let created = bootstrap_control_plane(&bootstrap_dir, &config)?;
    commit_and_push_bootstrap(&bootstrap_dir, &created, &ensured.repo.default_branch)?;

    let config_path = config.save()?;

    println!("Saved EnvCraft config to {}", config_path.display());
    if ensured.created {
        println!(
            "Created private control-plane repository at {}",
            ensured.repo.html_url
        );
    } else {
        println!(
            "Using existing control-plane repository at {}",
            ensured.repo.html_url
        );
    }
    println!("Bootstrapped control plane at {}", bootstrap_dir.display());
    for file in created {
        println!("  wrote {}", file.display());
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
    if let Some(config) = AppConfig::load_optional()? {
        let synced_path = sync_control_plane_project_schema(&config, &schema)?;
        println!("Synced control-plane schema at {}", synced_path.display());
    }
    println!("Linked project with schema at {}", path.display());
    Ok(())
}

fn set(args: SetArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let root = args.root;
    let mut schema = load_schema_for_write(&root, args.project.as_deref(), &args.environment)?;
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

    let secret_name = schema.secret_name_for(&args.logical_key, &args.environment);
    let github = GitHubClient::from_config(&config)?;
    github.put_repo_secret(
        &config.github_owner,
        &config.control_repo,
        &secret_name,
        &value,
    )?;
    let path = schema.save_to(&root)?;
    let synced_path = sync_control_plane_project_schema(&config, &schema)?;
    println!("Stored {}", secret_name);
    println!("Schema updated at {}", path.display());
    println!("Control-plane schema synced at {}", synced_path.display());
    Ok(())
}

fn generate(args: GenerateArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let github = GitHubClient::from_config(&config)?;
    let root = args.root;
    let mut schema = load_schema_for_write(&root, args.project.as_deref(), &args.environment)?;
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
            let secret_name = schema.secret_name_for(logical_key, &args.environment);
            github.put_repo_secret(
                &config.github_owner,
                &config.control_repo,
                &secret_name,
                value,
            )?;
        }
    }

    let path = schema.save_to(&root)?;
    let synced_path = sync_control_plane_project_schema(&config, &schema)?;
    println!(
        "Generated {} secrets for {}:{}",
        vars.len(),
        schema.project,
        args.environment
    );
    println!("Schema updated at {}", path.display());
    println!("Control-plane schema synced at {}", synced_path.display());
    Ok(())
}

fn list(args: ListArgs) -> Result<()> {
    let schema = load_schema_for_read(&args.root, args.project.as_deref())?;
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
            Some(environment) => schema.secret_name_for(logical_key, environment),
            None => spec.display_secret_name(&schema.project, logical_key, None),
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

fn upgrade(args: UpgradeArgs) -> Result<()> {
    let version_label = args.version.as_deref().unwrap_or("latest");
    let installed_path = upgrade_binary(args.version.as_deref())?;
    println!(
        "Upgraded EnvCraft to {} at {}",
        version_label,
        installed_path.display()
    );
    println!("Run `envcraft --version` to confirm the active binary.");
    Ok(())
}

fn pull(args: DeliverArgs) -> Result<()> {
    let config = AppConfig::load()?;
    let github = GitHubClient::from_config(&config)?;
    let schema = load_schema_for_read(&args.root, args.project.as_deref())?;
    let mut env_map = BTreeMap::new();

    for (logical_key, _) in schema.keys() {
        let session = DeliverySession::new();
        session.save()?;
        let secret_name = schema.secret_name_for(logical_key, &args.environment);
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
    let schema = load_schema_for_read(&args.root, args.project.as_deref())?;

    if !schema.vars.contains_key(&args.logical_key) {
        bail!("{} is not declared in .envcraft.schema", args.logical_key);
    }

    let session = DeliverySession::new();
    session.save()?;
    let secret_name = schema.secret_name_for(&args.logical_key, &args.environment);
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
    let schema = load_schema_for_read(&args.root, args.project.as_deref())?;
    let mut env_map = BTreeMap::new();

    for (logical_key, _) in schema.keys() {
        let session = DeliverySession::new();
        session.save()?;
        let secret_name = schema.secret_name_for(logical_key, &args.environment);
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

fn load_schema_for_read(root: &Path, project_override: Option<&str>) -> Result<ProjectSchema> {
    let mut schema = ProjectSchema::load_from(root).with_context(|| {
        format!(
            "missing or invalid {} in {}",
            crate::schema::DEFAULT_SCHEMA_FILE,
            root.display()
        )
    })?;

    if let Some(project) = project_override {
        schema.project = project.to_string();
    }

    Ok(schema)
}

fn load_schema_for_write(
    root: &Path,
    project_override: Option<&str>,
    environment: &str,
) -> Result<ProjectSchema> {
    match ProjectSchema::load_from(root) {
        Ok(mut schema) => {
            if let Some(project) = project_override {
                schema.project = project.to_string();
            }
            schema.ensure_environment(environment);
            Ok(schema)
        }
        Err(error) => {
            let project = project_override.ok_or_else(|| {
                anyhow!(
                    "missing .envcraft.schema in {}. Pass --project to create the context or run envcraft link first",
                    root.display()
                )
            })?;
            let mut schema = ProjectSchema::new(project.to_string(), [environment.to_string()]);
            schema.ensure_environment(environment);
            let _ = error;
            Ok(schema)
        }
    }
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

fn ensure_local_control_repo(root: &Path, clone_url: &str, default_branch: &str) -> Result<()> {
    if root.join(".git").exists() {
        run_git(root, &["pull", "--ff-only"])?;
        return Ok(());
    }

    if root.exists() && fs::read_dir(root)?.next().is_some() {
        bail!(
            "bootstrap directory {} exists and is not an empty git repository",
            root.display()
        );
    }

    if let Some(parent) = root.parent() {
        fs::create_dir_all(parent)?;
    }

    run_git_in(None, &["clone", clone_url, &root.display().to_string()])?;

    if repo_has_no_commits(root)? {
        run_git(root, &["checkout", "-B", default_branch])?;
    }

    Ok(())
}

fn sync_control_plane_project_schema(
    config: &AppConfig,
    schema: &ProjectSchema,
) -> Result<PathBuf> {
    let control_repo_root = config.control_repo_path()?;
    if !control_repo_root.join(".git").exists() {
        bail!(
            "control-plane repository is not available at {}. Run `envcraft init` first",
            control_repo_root.display()
        );
    }

    let project_dir = control_repo_root.join("projects").join(&schema.project);
    fs::create_dir_all(&project_dir)?;
    let schema_path = schema.save_to(&project_dir)?;
    commit_and_push_paths(
        &control_repo_root,
        &[schema_path.clone()],
        &config.default_ref,
        &format!("Sync EnvCraft schema for {}", schema.project),
    )?;
    Ok(schema_path)
}

fn commit_and_push_bootstrap(
    root: &Path,
    created_files: &[PathBuf],
    default_branch: &str,
) -> Result<()> {
    commit_and_push_paths(
        root,
        created_files,
        default_branch,
        "Bootstrap EnvCraft control plane",
    )
}

fn commit_and_push_paths(
    root: &Path,
    files: &[PathBuf],
    default_branch: &str,
    commit_message: &str,
) -> Result<()> {
    if repo_has_no_commits(root)? {
        run_git(root, &["checkout", "-B", default_branch])?;
    }

    let relative_files = files
        .iter()
        .map(|path| {
            path.strip_prefix(root)
                .unwrap_or(path)
                .display()
                .to_string()
        })
        .collect::<Vec<_>>();

    if !relative_files.is_empty() {
        let mut add_command = ProcessCommand::new("git");
        add_command.arg("-C").arg(root);
        add_command.arg("add");
        for path in &relative_files {
            add_command.arg(path);
        }
        let output = add_command
            .output()
            .context("failed to stage bootstrap files")?;
        if !output.status.success() {
            bail!(
                "git add failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
    }

    let status = run_git_capture(root, &["status", "--short"])?;
    if status.trim().is_empty() {
        return Ok(());
    }

    run_git(root, &["commit", "-m", commit_message])?;
    run_git(root, &["push", "-u", "origin", default_branch])?;
    Ok(())
}

fn repo_has_no_commits(root: &Path) -> Result<bool> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .context("failed to inspect git history")?;
    Ok(!output.status.success())
}

fn run_git(root: &Path, args: &[&str]) -> Result<()> {
    let _ = run_git_capture(root, args)?;
    Ok(())
}

fn run_git_capture(root: &Path, args: &[&str]) -> Result<String> {
    run_git_in(Some(root), args)
}

fn run_git_in(root: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut command = ProcessCommand::new("git");
    if let Some(root) = root {
        command.arg("-C").arg(root);
    }
    command.args(args);
    let output = command.output().context("failed to run git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::{config::AppConfig, schema::ProjectSchema};

    use super::{load_schema_for_read, load_schema_for_write, write_dotenv};

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

    #[test]
    fn load_schema_for_read_supports_project_override() {
        let dir = tempdir().unwrap();
        let schema = ProjectSchema::new("nui-app", ["dev".to_string()]);
        schema.save_to(dir.path()).unwrap();

        let loaded = load_schema_for_read(dir.path(), Some("override-app")).unwrap();
        assert_eq!(loaded.project, "override-app");
    }

    #[test]
    fn load_schema_for_write_can_bootstrap_from_override() {
        let dir = tempdir().unwrap();
        let loaded = load_schema_for_write(dir.path(), Some("manual-project"), "prod").unwrap();
        assert_eq!(loaded.project, "manual-project");
        assert!(loaded.environments.contains("prod"));
    }
}
