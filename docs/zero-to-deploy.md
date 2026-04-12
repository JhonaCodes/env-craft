# EnvCraft 0 to 100

This is the shortest end-to-end setup path for EnvCraft.

It covers:

- first-time control-plane setup
- linking one project
- storing secrets
- GitHub Actions builds
- remote server or Dokploy deploys

Use this document when you want the full operational flow instead of isolated command docs.

## Two names you must not mix

EnvCraft usually deals with two different names:

- **GitHub repo slug**
  - example: `my-app`
  - used for `--ci-repo` when seeding Actions secrets for a repository
- **EnvCraft project slug**
  - example: `my_app`
  - used inside `.envcraft.schema` and with `--project`

## Prerequisites

- `envcraft` installed globally
- GitHub auth available locally through `gh auth login` or `GITHUB_TOKEN`
- a private control-plane repo name, for example `envcraft-secrets`
- a target application repo, for example `my-org/my-app`

## Phase 1: Bootstrap the control plane

Run this once:

```bash
envcraft init --github-owner my-org --control-repo envcraft-secrets
```

This does:

- creates the private control-plane repo if it does not exist
- clones it locally under `~/.envcraft/repos/envcraft-secrets`
- writes the delivery workflow and script
- writes control-plane notes under `.envcraft/`
- saves global config in `~/.envcraft/config.toml`

After this, the control plane exists, but CI auth is not finished yet.

## Phase 2: Register the GitHub App for CI

Run:

```bash
envcraft github-app setup
```

This does:

- starts the GitHub App manifest flow
- registers one GitHub App for this control plane
- stores the App ID and PEM locally under `~/.envcraft/github-apps/`
- defaults to owner installation mode `all`

If you want a selected-repositories installation instead:

```bash
envcraft github-app setup \
  --install-mode selected \
  --install-repo my-org/envcraft-secrets \
  --install-repo my-org/my-app
```

To connect more CI repositories to the same app later:

```bash
envcraft github-app connect --ci-repo my-app
```

`connect` is the repo-onboarding command. It is meant to:

- attach the repo to the existing GitHub App installation
- seed `ENVCRAFT_GITHUB_APP_ID` and `ENVCRAFT_GITHUB_APP_PRIVATE_KEY` into that repo
- fall back to the GitHub installation configure page when GitHub rejects the attach API

There is still one one-time GitHub step that cannot be skipped:

- complete the install or configure page that `setup` opens or prints
- make sure `my-org/envcraft-secrets` is included

Validate:

```bash
envcraft github-app status
```

## Phase 3: Link one project repository

Inside the project repo:

```bash
cd /path/to/my_app
envcraft link --project my_app --env dev --env prod
```

This creates `.envcraft.schema`.

That file is the contract for:

- which logical variables exist
- which environments exist
- how EnvCraft resolves names for GitHub Secrets

## Phase 4: Add secrets

Examples:

```bash
envcraft set API_BASE_URL --env dev
envcraft set API_BASE_URL --env prod
envcraft set LEGAL_BASE_URL --env prod --value https://legal.example.com
envcraft set DB_PASSWORD --env prod --generate
```

What `envcraft set` does:

- creates or updates the GitHub Secret in the control-plane repo
- updates local `.envcraft.schema`
- syncs the schema into `envcraft-secrets/projects/<project>/.envcraft.schema`

Verify:

```bash
envcraft list --remote --env prod
```

## Phase 5A: Build with GitHub Actions

Use this path when your application is compiled in GitHub Actions.

Typical use case:

- Flutter
- frontend builds
- any project where values become part of the build output

Workflow pattern:

```yaml
- name: Install EnvCraft
  run: curl -fsSL https://raw.githubusercontent.com/JhonaCodes/env-craft/main/install.sh | bash

- name: Resolve build env
  env:
    ENVCRAFT_GITHUB_OWNER: my-org
    ENVCRAFT_CONTROL_REPO: envcraft-secrets
    ENVCRAFT_GITHUB_APP_ID: ${{ secrets.ENVCRAFT_GITHUB_APP_ID }}
    ENVCRAFT_GITHUB_APP_PRIVATE_KEY: ${{ secrets.ENVCRAFT_GITHUB_APP_PRIVATE_KEY }}
  run: |
    envcraft pull --env prod --project my_app --root . --output .env
```

Then continue with your normal build:

```yaml
- name: Generate code
  run: dart run build_runner build --delete-conflicting-outputs

- name: Build app
  run: flutter build appbundle --release
```

Use `pull` in CI when:

- the app needs a `.env` during compilation
- values are part of the generated or compiled artifact

Do **not** use `deploy-inject` for that case.

## Phase 5B: Deploy to a remote server or Dokploy

Use this path when the application runs on a remote host and should receive secrets right before runtime.

Typical use case:

- API
- worker
- Docker container runtime
- Dokploy prestart/init hook

Install `envcraft` on the server first.

Important:

- V1 currently resolves one workflow run per key
- use this for one-shot hooks, not for a container `ENTRYPOINT` that may restart multiple times
- if Dokploy can store service environment variables directly, prefer resolving the environment once and saving the resulting values in the service config

Then run:

```bash
envcraft deploy-inject --env prod > /tmp/envcraft.env.sh
source /tmp/envcraft.env.sh
./your-process
```

Or inside a deploy script:

```bash
#!/usr/bin/env bash
set -euo pipefail

envcraft deploy-inject --env prod > /tmp/myapp.env.sh
source /tmp/myapp.env.sh
exec docker compose up -d
```

For non-interactive remote servers, provide CI-style auth:

- `ENVCRAFT_GITHUB_APP_ID`
- `ENVCRAFT_GITHUB_APP_PRIVATE_KEY`

Use `deploy-inject` when:

- you want secrets at runtime, not build time
- you do not want secrets baked into Docker layers
- Dokploy or the host is the deploy executor

Do **not** use `deploy-inject` inside a `Dockerfile`.
Do **not** use `deploy-inject` inside a long-lived API container startup script that may be retried automatically.

## Which command to use

Use `pull` when:

- the build step needs `.env`
- the values are consumed during compilation
- the result is a file like `.env`, `.env.dev`, or `.env.prod`

Use `deploy-inject` when:

- the process needs exported environment variables right before it starts
- the target is a server, container runtime, or Dokploy hook

Prefer resolving once into platform-managed environment variables when:

- the runtime is restart-prone
- the service healthcheck may fail while secrets are still being resolved
- the deployment platform already has a durable environment store

## First real test path

If you want the shortest possible proof that EnvCraft works:

1. `envcraft init --github-owner my-org --control-repo envcraft-secrets`
2. `envcraft github-app setup`
3. or `envcraft github-app setup --install-mode selected --install-repo my-org/envcraft-secrets --install-repo my-org/my-app`
4. complete the one-time install or configure page from `setup` on `my-org/envcraft-secrets`
5. `envcraft github-app connect --ci-repo my-app` when the first CI repo needs the app
6. `envcraft github-app connect --ci-repo another-app` when another CI repo needs the same app
7. `cd /path/to/my_app`
8. `envcraft link --project my_app --env dev --env prod`
9. `envcraft set API_BASE_URL --env prod`
10. `envcraft reveal API_BASE_URL --env prod`
11. wire `envcraft pull --env prod --output .env` into GitHub Actions
12. run one successful build

For a remote-server proof:

1. install `envcraft` on the host
2. ensure the host has GitHub App credentials
3. run `envcraft deploy-inject --env prod`
4. source the output
5. start the process

## Common mistakes

- Using `my_app` as the GitHub repo slug in `--ci-repo`
- Using `my-app` as the EnvCraft project slug in `--project`
- Assuming `init` also finishes CI auth
- Creating a new GitHub App for every project instead of using `connect` to attach more CI repos
- Forgetting the one-time owner installation on the control-plane repo before the first `connect`
- Assuming `selected` mode never needs the browser again when adding later repos
- Using `deploy-inject` in a `Dockerfile`
- Expecting `pull` to mutate secrets; it only reads through GitHub Actions
