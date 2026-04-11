# `envcraft pull`

## Purpose

Build a local `.env` file for one environment by resolving every declared key through GitHub Actions.

## When to use it

- prepare local development quickly
- rebuild a missing `.env.dev`
- confirm that all declared keys can be delivered

## Syntax

```bash
envcraft pull --env <env> [--output <path>]
```

## Canonical examples

Build `.env.dev` explicitly:

```bash
envcraft pull --env dev --output .env.dev
```

Use the default output path:

```bash
envcraft pull --env prod
```

Run outside the repo:

```bash
envcraft pull \
  --env dev \
  --project my_app \
  --root /path/to/my_app \
  --output /tmp/my_app.env
```

## Expected side effects

- dispatches one GitHub Actions delivery per logical key
- shows the delivery spinner while waiting
- writes the final output file locally

## Authentication notes

Interactive local usage:

- EnvCraft can use your local `gh` session automatically
- or an explicit `GITHUB_TOKEN`

GitHub Actions usage:

- prefer `ENVCRAFT_GITHUB_APP_ID` plus `ENVCRAFT_GITHUB_APP_PRIVATE_KEY` or `ENVCRAFT_GITHUB_APP_PRIVATE_KEY_FILE`
- if the workflow is in a different private repo than the control-plane repo, install the GitHub App on the control-plane repo
- `GITHUB_TOKEN` and `ENVCRAFT_GITHUB_TOKEN` remain legacy fallbacks

Example in GitHub Actions:

```yaml
- name: Resolve build env with EnvCraft
  env:
    ENVCRAFT_GITHUB_APP_ID: ${{ secrets.ENVCRAFT_GITHUB_APP_ID }}
    ENVCRAFT_GITHUB_APP_PRIVATE_KEY: ${{ secrets.ENVCRAFT_GITHUB_APP_PRIVATE_KEY }}
  run: |
    envcraft pull --env prod --project my_app --root . --output .env
```

## Common mistakes

- expecting `pull` to modify GitHub secrets; it only reads them through Actions
