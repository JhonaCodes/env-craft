# EnvCraft

EnvCraft is a Rust 2024 CLI for managing environment variables across many projects while keeping **GitHub Secrets** as the only secret store.

It is meant for the situation where:
- one person or a small team owns many repositories
- local `.env` setup is repetitive and error-prone
- deploy secrets should not live only inside Dokploy
- GitHub should remain the long-term source of truth, while GitHub Actions performs controlled secret delivery

The intended usage model is:
- install the `envcraft` binary globally
- run `envcraft` from any project directory
- let the current directory's `.envcraft.schema` resolve the project by default
- use `--root` or `--project` only when you need to operate from somewhere else or recover from bad local context

V1 goals in this repository:
- bootstrap a central control-plane repo managed by EnvCraft
- keep `.envcraft.schema` as the contract for each project
- create and update secrets in GitHub
- reveal one secret at a time through **GitHub Actions**
- build local `.env` files and deploy-time export scripts from per-key delivery

## What EnvCraft does

- stores secret values in GitHub Secrets, not in the CLI repository
- uses the current repository context to determine which project you are operating on
- supports explicit overrides with `--project` and `--root` when you need to work from another directory
- uses GitHub Actions for all secret read operations because GitHub does not expose secret values directly through its API
- produces local `.env` files for development and shell exports for deploy-time injection

## What EnvCraft does not do

- it does not read secret values directly through GitHub's REST API
- it does not bake secrets into Docker images by design
- it does not require a custom database or custom vault service for V1
- it does not replace Dokploy; it complements Dokploy by handling secret delivery before runtime

## Current V1 transport

This first implementation uses:
- GitHub Secrets as storage
- GitHub Actions as the only authorized secret reader
- encrypted one-time payloads returned as workflow artifacts

That keeps the system usable without additional public socket infrastructure while preserving the core rule: secret values are only read inside Actions.

## Main commands

```bash
cargo install --path .

envcraft init \
  --github-owner JhonaCodes \
  --control-repo envcraft-secrets \
  --bootstrap-dir /path/to/envcraft-secrets

envcraft link --project nui-app --env dev --env prod

envcraft set DB_PASSWORD --env prod --generate

envcraft generate --env dev --preset postgres --preset jwt

envcraft list --remote

envcraft reveal DB_PASSWORD --env prod

envcraft pull --env dev --output .env.dev

envcraft deploy-inject --env prod > env.sh

# Explicit override when running from another directory
envcraft set DB_PASSWORD --env prod --project nui-app --root /path/to/nui-app --generate
```

## Typical workflows

### 1. Bootstrap the control plane

```bash
envcraft init \
  --github-owner JhonaCodes \
  --control-repo envcraft-secrets \
  --bootstrap-dir ~/code/envcraft-secrets
```

This creates the global EnvCraft config in `~/.envcraft/config.toml` and optionally writes the control-plane workflow files into a local checkout of `envcraft-secrets`.

### 2. Link an application repository

```bash
cd ~/code/nui-app
envcraft link --project nui-app --env dev --env prod
```

That creates `.envcraft.schema`, which becomes the local contract for the repository.

### 3. Create or rotate secrets

```bash
envcraft set DB_PASSWORD --env prod --generate
envcraft set STRIPE_SECRET_KEY --env prod
```

These commands write to GitHub Secrets in the central control-plane repository and sync the local schema metadata.

### 4. Build local developer env files

```bash
envcraft pull --env dev --output .env.dev
```

Each requested logical key is resolved through a one-time GitHub Actions workflow and assembled into a local `.env` file.

### 5. Inject secrets for deployment

```bash
envcraft deploy-inject --env prod > env.sh
source env.sh
```

This is the intended V1 integration point for Dokploy prestart or init hooks: Dokploy still builds and deploys, while EnvCraft resolves secrets right before runtime.

## Release installation

GitHub Actions builds release binaries from tags and uploads them to GitHub Releases.

For a private repository, install from an authenticated GitHub CLI session:

```bash
mkdir -p /tmp/envcraft-install ~/.local/bin
gh release download v0.1.0 \
  --repo JhonaCodes/env-craft \
  --pattern 'envcraft-macos-aarch64.tar.gz' \
  --dir /tmp/envcraft-install
tar -xzf /tmp/envcraft-install/envcraft-macos-aarch64.tar.gz -C /tmp/envcraft-install
install -m 0755 /tmp/envcraft-install/envcraft ~/.local/bin/envcraft
envcraft --version
```

If `~/.local/bin` is not on your `PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Supported release assets:
- `envcraft-linux-x86_64.tar.gz`
- `envcraft-macos-x86_64.tar.gz`
- `envcraft-macos-aarch64.tar.gz`

If the repository is made public later, the helper script can be used directly:

```bash
curl -fsSL https://raw.githubusercontent.com/JhonaCodes/env-craft/main/scripts/install-from-github.sh | VERSION=v0.1.0 bash
```

To publish a release, push a semantic version tag such as `v0.1.0`.

## Control-plane bootstrap

`envcraft init --bootstrap-dir ...` writes:
- `.github/workflows/deliver.yml`
- `.github/scripts/envcraft-deliver.mjs`
- `projects/.gitkeep`

The generated workflow handles **single-key delivery**. `pull` and `deploy-inject` assemble the final payload by requesting each declared key independently.

## Environment contract

Each application repo uses `.envcraft.schema`:

```yaml
project: nui-app
environments:
  - dev
  - prod
vars:
  DB_PASSWORD:
    vault_key: NUI_APP_PROD_DB_PASSWORD
    type: secret
    generate: true
    required: true
```

## Notes

- GitHub does not expose secret values through its API.
- Because of that, EnvCraft writes secrets directly through the GitHub Secrets API, but reads them only through Actions.
- Full `.env` delivery currently costs one workflow run per key. That is acceptable for V1 correctness and can be optimized later.
- The CLI is intended to be installed globally; the source repository is only for development and release publishing.
