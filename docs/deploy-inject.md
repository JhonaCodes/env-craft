# `envcraft deploy-inject`

## Purpose

Emit shell exports for deploy-time injection without baking secrets into images.

## When to use it

- prestart or init hooks
- deploy pipelines that need runtime environment exports
- one-shot deploy scripts on remote hosts

Prefer another pattern when:

- your container can restart automatically while unhealthy
- your process manager re-runs the startup script multiple times
- your platform can persist service environment variables directly

In those cases, resolve the environment once and store the final values in the platform instead of calling `deploy-inject` in the application startup path.

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
envcraft deploy-inject --env prod --output /tmp/my-app-prod-env.sh
```

Run outside the repo:

```bash
envcraft deploy-inject \
  --env prod \
  --project my_app \
  --root /path/to/my_app
```

## Expected behavior

- resolves every declared key for the selected environment
- shows the delivery spinner while waiting
- outputs `export KEY='value'` lines

## Authentication notes

- local shells usually do not need dedicated CI auth because EnvCraft can use your interactive GitHub auth
- in GitHub Actions or another non-interactive CI environment, prefer `ENVCRAFT_GITHUB_APP_ID` plus `ENVCRAFT_GITHUB_APP_PRIVATE_KEY` or `ENVCRAFT_GITHUB_APP_PRIVATE_KEY_FILE`
- if `~/.envcraft/config.toml` is not present in that CI environment, also set:
  - `ENVCRAFT_GITHUB_OWNER`
  - `ENVCRAFT_CONTROL_REPO`
- this is needed when that workflow must access a separate private control-plane repo such as `envcraft-secrets`
- if a repository never runs EnvCraft in CI, it does not need these values

## Common mistakes

- using this in a Dockerfile build stage; it is intended for runtime or prestart injection
- using this in the main `ENTRYPOINT` of an API container that may restart repeatedly
- assuming every repository needs GitHub App CI auth; it is a CI integration requirement, not a universal requirement
