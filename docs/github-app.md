# GitHub App CI Auth

## Purpose

Use the EnvCraft GitHub App when a repository runs `envcraft` inside GitHub Actions and needs temporary access to a private control-plane repository such as `envcraft-secrets`.

This is the preferred CI auth path.

## Why this exists

`GITHUB_TOKEN` from one repository cannot read another private repository by default.

EnvCraft solves that by using:

- a GitHub App installed on the control-plane repo
- a short-lived installation access token minted at runtime
- repository secrets in the CI repo that provide the App ID and PEM

## Command

```bash
envcraft github-app setup
envcraft github-app setup --install-mode selected --install-repo my-org/envcraft-secrets --install-repo my-org/my-app
envcraft github-app connect --ci-repo another-app
```

Important:

- `--ci-repo` expects the **GitHub repo slug**
- for example `my-app`
- that is different from the EnvCraft project slug `my_app`

## What `setup` does

- starts the GitHub App manifest flow locally
- registers one GitHub App from a manifest if none exists yet
- stores the App ID and PEM under `~/.envcraft/github-apps/`
- stores the desired install mode:
  - `all`
  - `selected`
- stores the selected install repos when `--install-mode selected` is used
- opens the install or configure page unless `--no-open`
- verifies the owner installation state after the browser step when possible

Defaults:

- `setup` defaults to `--install-mode all`
- in `selected` mode, the control-plane repo is always required and EnvCraft adds it automatically if you omit it

If the app already exists locally, `setup` reuses it and only prints the existing app details.

If the app exists locally but was deleted in GitHub, `setup` detects the stale local state, removes it, and starts a fresh registration flow.

## What `connect` does

- reuses the existing locally stored GitHub App
- attaches the CI repo to the existing GitHub App installation when possible
- writes these Actions secrets into additional CI repos:
  - `ENVCRAFT_GITHUB_APP_ID`
  - `ENVCRAFT_GITHUB_APP_PRIVATE_KEY`
- updates the locally stored metadata so `status` can list connected repos

Important:

- `connect` only works after the GitHub App has a real owner-level installation
- if GitHub rejects the attach API, EnvCraft opens the installation configure page and waits for the repo to appear before seeding CI secrets
- if the repo never appears before timeout, EnvCraft fails without marking the repo as connected

## What you still need to do

After `envcraft github-app setup` completes:

1. Complete the owner installation that `setup` opens or prints.
2. If you chose `selected` mode, make sure the requested repos are selected.
3. Run `envcraft github-app connect --ci-repo my-app`.
4. Confirm the CI repo now has:
   - `ENVCRAFT_GITHUB_APP_ID`
   - `ENVCRAFT_GITHUB_APP_PRIVATE_KEY`
5. Run:

```bash
envcraft github-app status
```

## Canonical example

```bash
envcraft init --github-owner my-org --control-repo envcraft-secrets
envcraft github-app setup
envcraft github-app setup --install-mode selected --install-repo my-org/envcraft-secrets --install-repo my-org/my-app
envcraft github-app connect --ci-repo my-app
envcraft github-app connect --ci-repo another-app
envcraft github-app status
```

Then, in a project workflow:

```yaml
env:
  ENVCRAFT_GITHUB_APP_ID: ${{ secrets.ENVCRAFT_GITHUB_APP_ID }}
  ENVCRAFT_GITHUB_APP_PRIVATE_KEY: ${{ secrets.ENVCRAFT_GITHUB_APP_PRIVATE_KEY }}
run: |
  envcraft pull --env prod --project my_app --root . --output .env
```

## Local storage

EnvCraft stores the local GitHub App credentials here:

- `~/.envcraft/github-apps/<owner>-<control-repo>.toml`
- `~/.envcraft/github-apps/<owner>-<control-repo>.pem`

Those files are for local reuse only. CI should still use repository secrets.

## What `status` shows

`envcraft github-app status` shows:

- the stored App ID
- the stored slug
- the install URL
- the stored install mode
- the requested selected repos from setup
- the local private key path
- whether the control-plane repo is actually attached
- the owner installation id and repository selection mode
- the repositories currently attached to the installation
- the CI repositories already connected through `connect`

## Common mistakes

- Assuming every repo needs GitHub App secrets. Only repos that run EnvCraft in CI need them.
- Running `setup` as if it should create a separate GitHub App per project. EnvCraft should use one app per control plane, then `connect` more repos to it.
- Deleting the GitHub App in GitHub and expecting old local metadata to keep working. Re-run `envcraft github-app setup` so EnvCraft can recreate it.
- Skipping the one-time owner installation on the control-plane repo and expecting `connect` to work.
- Expecting `selected` mode to auto-attach repos without either compatible auth or the browser configure step.
- Keeping only `ENVCRAFT_GITHUB_TOKEN` and forgetting to migrate the workflow env vars.
- Expecting `envcraft init` alone to finish CI auth. `init` bootstraps the control plane; `github-app setup` finishes the CI auth path.
