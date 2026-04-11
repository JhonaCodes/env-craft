# `envcraft set`

## Purpose

Create or update one logical variable for one environment.

## When to use it

- add a brand-new variable
- update a value
- create matching `dev` and `prod` variants
- generate a secret automatically

## Syntax

```bash
envcraft set <LOGICAL_KEY> --env <env>
```

## Reads

- local `.envcraft.schema` by default
- or `--project` / `--root` overrides

## Writes

- one GitHub Secret in the control-plane repo
- local `.envcraft.schema`
- synced control-plane schema in `projects/<project>/.envcraft.schema`

## Canonical examples

Set a dev value explicitly:

```bash
envcraft set API_BASE_URL --env dev --value https://api-dev.example.com
```

Set a prod value explicitly:

```bash
envcraft set API_BASE_URL --env prod --value https://api.example.com
```

Prompt interactively:

```bash
envcraft set STRIPE_SECRET_KEY --env prod
```

Generate a secret automatically:

```bash
envcraft set JWT_SECRET --env prod --generate
```

Run outside the repo:

```bash
envcraft set API_BASE_URL \
  --env dev \
  --project my_app \
  --root /path/to/my_app \
  --value https://api-dev.example.com
```

## Expected side effects

- stores a remote secret like `MY_APP_DEV_API_BASE_URL`
- updates the project schema so the variable exists for that environment
- syncs the project schema into the control-plane repo

## Common mistakes

- setting only `dev` and expecting `prod` to exist automatically
- forgetting `--root` when running outside the repo

## Related commands

- [list](list.md)
- [reveal](reveal.md)
- [pull](pull.md)
