# EnvCraft

EnvCraft is a Rust 2024 CLI for managing environment variables across many projects while keeping **GitHub Secrets** as the only secret store.

## Quick install

Install the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/JhonaCodes/env-craft/main/install.sh | bash
```

Install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/JhonaCodes/env-craft/main/install.sh | VERSION=v0.1.6 bash
```

Verify the binary:

```bash
envcraft --version
```

Update later without reinstalling manually:

```bash
envcraft upgrade
```

Or pin a version explicitly:

```bash
envcraft upgrade --version v0.1.6
```

If `envcraft` is not found after installation:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

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

## Documentation

Start here:

- [0 to 100 setup and deploy flow](docs/zero-to-deploy.md)
- [Documentation index](docs/README.md)
- [Mental model](docs/mental-model.md)
- [Context resolution](docs/context-resolution.md)
- [GitHub App CI auth](docs/github-app.md)
- [Future: migration and import flows](docs/future-migration.md)

Command reference:

- [init](docs/init.md)
- [link](docs/link.md)
- [set](docs/set.md)
- [generate](docs/generate.md)
- [list](docs/list.md)
- [github-app](docs/github-app.md)
- [reveal](docs/reveal.md)
- [pull](docs/pull.md)
- [deploy-inject](docs/deploy-inject.md)
- [upgrade](docs/upgrade.md)

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
envcraft init \
  --github-owner JhonaCodes \
  --control-repo envcraft-secrets \
  --bootstrap-dir /path/to/envcraft-secrets

envcraft github-app setup --ci-repo my-app

envcraft link --project nui-app --env dev --env prod

envcraft set DB_PASSWORD --env prod --generate

envcraft generate --env dev --preset postgres --preset jwt

envcraft list --remote

envcraft upgrade

envcraft reveal DB_PASSWORD --env prod

envcraft pull --env dev --output .env.dev

envcraft deploy-inject --env prod > env.sh

# Explicit override when running from another directory
envcraft set DB_PASSWORD --env prod --project nui-app --root /path/to/nui-app --generate
```

For the complete command contract and more variants, use the docs in [`docs/`](docs/README.md).

## 0 to 100 flow

If you want the shortest full path from a fresh install to a working deploy, read:

- [0 to 100 setup and deploy flow](docs/zero-to-deploy.md)

That guide covers:

- control-plane bootstrap
- GitHub App setup
- linking a project
- storing secrets
- building in GitHub Actions
- deploying on a remote server or Dokploy

## Future functionality

Planned, but not part of the current implementation:

- [Future: migration and import flows](docs/future-migration.md)

## Typical workflows

### 1. Bootstrap the control plane

```bash
envcraft init \
  --github-owner JhonaCodes \
  --control-repo envcraft-secrets \
  --bootstrap-dir ~/code/envcraft-secrets
```

This single command now:
- reads your GitHub auth from `GITHUB_TOKEN`
- or prefers GitHub App credentials from `ENVCRAFT_GITHUB_APP_ID` plus `ENVCRAFT_GITHUB_APP_PRIVATE_KEY` / `ENVCRAFT_GITHUB_APP_PRIVATE_KEY_FILE`
- or falls back to your local `gh` session automatically from inside `envcraft`
- creates the private `envcraft-secrets` repository if it does not exist
- clones or updates the local control-plane checkout
- writes the control-plane workflow files
- commits and pushes the bootstrap if there are changes
- saves the global EnvCraft config in `~/.envcraft/config.toml`

To finish the CI auth path after `init`, run:

```bash
envcraft github-app setup --ci-repo my-app
```

That command registers the GitHub App, stores the App ID and PEM locally, and can seed the CI repository secrets automatically.

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

CI note:
- only repositories or workflows that run `envcraft` inside GitHub Actions against a private control-plane repo need dedicated non-interactive auth
- prefer `ENVCRAFT_GITHUB_APP_ID` plus `ENVCRAFT_GITHUB_APP_PRIVATE_KEY` or `ENVCRAFT_GITHUB_APP_PRIVATE_KEY_FILE`
- `ENVCRAFT_GITHUB_TOKEN` is a legacy fallback
- local development usually does not need this because `envcraft` can use your interactive GitHub auth
- repositories that never run EnvCraft in CI do not need to add these values

### 5. Inject secrets for deployment

```bash
envcraft deploy-inject --env prod > env.sh
source env.sh
```

This is the intended V1 integration point for Dokploy prestart or init hooks: Dokploy still builds and deploys, while EnvCraft resolves secrets right before runtime.

If the deploy step runs inside GitHub Actions from another private repo, that workflow should prefer a GitHub App installation with access to the private control-plane repo.

## Release installation

GitHub Actions builds release binaries from tags and uploads them to GitHub Releases.

Public one-command installation:

```bash
curl -fsSL https://raw.githubusercontent.com/JhonaCodes/env-craft/main/install.sh | bash
```

Version-pinned installation:

```bash
curl -fsSL https://raw.githubusercontent.com/JhonaCodes/env-craft/main/install.sh | VERSION=v0.1.6 bash
```

Supported release assets:
- `envcraft-linux-x86_64.tar.gz`
- `envcraft-macos-x86_64.tar.gz`
- `envcraft-macos-aarch64.tar.gz`

To publish a release, push a semantic version tag such as `v0.1.6`.

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

## Where configuration lives

- `env-craft` repo:
  - source code for the CLI
  - release pipeline
  - installer script
- target application repo:
  - local `.envcraft.schema`
  - for example `my_app/.envcraft.schema`
- control-plane repo such as `envcraft-secrets`:
  - delivery workflow
  - control-plane script
  - GitHub Secrets storage
- local machine:
  - `~/.envcraft/config.toml`
  - `~/.envcraft/repos/envcraft-secrets`

## Notes

- GitHub does not expose secret values through its API.
- Because of that, EnvCraft writes secrets directly through the GitHub Secrets API, but reads them only through Actions.
- Full `.env` delivery currently costs one workflow run per key. That is acceptable for V1 correctness and can be optimized later.
- The CLI is intended to be installed globally; the source repository is only for development and release publishing.

## License

EnvCraft is licensed under the MIT License. See [LICENSE](LICENSE).

## Attribution

Forks, derivatives, and redistributions are welcome.

If you fork or redistribute EnvCraft, please keep a visible reference to the original repository:

`https://github.com/JhonaCodes/env-craft`
