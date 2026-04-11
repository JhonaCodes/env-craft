# `envcraft link`

## Purpose

Create or update `.envcraft.schema` for the current application repository.

## When to use it

- when onboarding a new repo into EnvCraft
- when adding a new environment like `staging`

## Syntax

```bash
envcraft link --project <project> --env <env> [--env <env> ...]
```

## Reads

- current directory by default
- or the directory passed through `--root`

## Writes

- local `.envcraft.schema`
- local `.gitignore`
- synced control-plane schema when EnvCraft global config already exists

## Common variants

Inside the repo:

```bash
cd /path/to/my_app
envcraft link --project my_app --env dev --env prod
```

Outside the repo:

```bash
envcraft link \
  --project my_app \
  --env dev \
  --env prod \
  --root /path/to/my_app
```

## Expected side effects

- creates `.envcraft.schema` if missing
- preserves and extends existing environments
- syncs `projects/<project>/.envcraft.schema` into the control-plane repo if initialized

## Common mistakes

- expecting `link` to create GitHub Secrets; it only creates the project contract
