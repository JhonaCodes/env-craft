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
- prints the install URL for the new app

If the app already exists locally, `setup` reuses it and only prints the existing app details.

If the app exists locally but was deleted in GitHub, `setup` detects the stale local state, removes it, and starts a fresh registration flow.

## What `connect` does

- reuses the existing locally stored GitHub App
- writes these Actions secrets into additional CI repos:
  - `ENVCRAFT_GITHUB_APP_ID`
  - `ENVCRAFT_GITHUB_APP_PRIVATE_KEY`
- updates the locally stored metadata so `status` can list connected repos

## What you still need to do

After `envcraft github-app setup` completes:

1. Open the install URL printed by EnvCraft.
2. Install the app on the control-plane repo, for example `my-org/envcraft-secrets`.
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
- the local private key path
- the CI repositories already connected through `connect`

## Common mistakes

- Assuming every repo needs GitHub App secrets. Only repos that run EnvCraft in CI need them.
- Running `setup` as if it should create a separate GitHub App per project. EnvCraft should use one app per control plane, then `connect` more repos to it.
- Deleting the GitHub App in GitHub and expecting old local metadata to keep working. Re-run `envcraft github-app setup` so EnvCraft can recreate it.
- Installing the app on the CI repo but not the control-plane repo.
- Keeping only `ENVCRAFT_GITHUB_TOKEN` and forgetting to migrate the workflow env vars.
- Expecting `envcraft init` alone to finish CI auth. `init` bootstraps the control plane; `github-app setup` finishes the CI auth path.
