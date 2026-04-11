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
  --project acordio_app \
  --root /path/to/acordio_app \
  --output /tmp/acordio.env
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

- the workflow must provide a token explicitly
- if the workflow is in a different private repo than the control-plane repo, add a secret such as `ENVCRAFT_GITHUB_TOKEN`
- map that secret into `GITHUB_TOKEN` for the EnvCraft step

Example in GitHub Actions:

```yaml
- name: Resolve build env with EnvCraft
  env:
    GITHUB_TOKEN: ${{ secrets.ENVCRAFT_GITHUB_TOKEN }}
  run: |
    envcraft pull --env prod --project acordio_app --root . --output .env
```

## Common mistakes

- expecting `pull` to modify GitHub secrets; it only reads them through Actions
