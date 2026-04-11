# `envcraft init`

## Purpose

Bootstrap or sync the global EnvCraft control-plane repository.

## When to use it

- first-time setup
- after upgrading EnvCraft when the control-plane workflow changed
- when you want to re-sync the control-plane repo

## Syntax

```bash
envcraft init --github-owner <owner> --control-repo <repo>
```

## Reads

- local GitHub auth from `GITHUB_TOKEN` or the local `gh` session

## Writes

- `~/.envcraft/config.toml`
- `~/.envcraft/repos/<control-repo>`
- the remote control-plane repo contents

## Common variants

Bootstrap with default local clone path:

```bash
envcraft init --github-owner JhonaCodes --control-repo envcraft-secrets
```

Bootstrap with an explicit local checkout path:

```bash
envcraft init \
  --github-owner JhonaCodes \
  --control-repo envcraft-secrets \
  --bootstrap-dir ~/code/envcraft-secrets
```

## Expected side effects

- creates the control-plane repo if needed
- clones or updates the local control-plane checkout
- writes `deliver.yml`
- writes `envcraft-deliver.mjs`
- commits and pushes bootstrap changes if needed

## Common mistakes

- assuming `init` links an application repo; that is `envcraft link`
- assuming `init` creates app secrets; that is `envcraft set`

