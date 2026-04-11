# `envcraft deploy-inject`

## Purpose

Emit shell exports for deploy-time injection without baking secrets into images.

## When to use it

- prestart or init hooks
- deploy pipelines that need runtime environment exports

## Syntax

```bash
envcraft deploy-inject --env <env>
```

## Canonical examples

Redirect exports into a script:

```bash
envcraft deploy-inject --env prod > env.sh
source env.sh
```

Write to an explicit file:

```bash
envcraft deploy-inject --env prod --output /tmp/acordio-prod-env.sh
```

Run outside the repo:

```bash
envcraft deploy-inject \
  --env prod \
  --project acordio_app \
  --root /path/to/acordio_app
```

## Expected behavior

- resolves every declared key for the selected environment
- shows the delivery spinner while waiting
- outputs `export KEY='value'` lines

## Authentication notes

- local shells usually do not need dedicated CI auth because EnvCraft can use your interactive GitHub auth
- in GitHub Actions or another non-interactive CI environment, prefer `ENVCRAFT_GITHUB_APP_ID` plus `ENVCRAFT_GITHUB_APP_PRIVATE_KEY` or `ENVCRAFT_GITHUB_APP_PRIVATE_KEY_FILE`
- this is needed when that workflow must access a separate private control-plane repo such as `envcraft-secrets`
- if a repository never runs EnvCraft in CI, it does not need these values

## Common mistakes

- using this in a Dockerfile build stage; it is intended for runtime or prestart injection
- assuming every repository needs GitHub App CI auth; it is a CI integration requirement, not a universal requirement
