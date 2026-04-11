# `envcraft list`

## Purpose

List the logical variables declared for a project and optionally show whether they exist remotely.

## When to use it

- inspect the local contract
- verify whether `dev` or `prod` values already exist in GitHub
- debug missing keys before `reveal` or `pull`

## Syntax

```bash
envcraft list [--remote] [--env <env>]
```

## Canonical examples

Show the local contract:

```bash
envcraft list
```

Check remote availability for one environment:

```bash
envcraft list --remote --env prod
```

Run outside the repo:

```bash
envcraft list \
  --remote \
  --env dev \
  --project acordio_app \
  --root /path/to/acordio_app
```

## Output

You will see:

- project slug
- environments
- each logical key
- the resolved secret name for the selected environment
- whether the remote secret exists

## Common mistakes

- expecting `list` to show secret values; it only shows metadata and availability

