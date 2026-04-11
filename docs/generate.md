# `envcraft generate`

## Purpose

Generate a set of standard secrets from stack presets.

## When to use it

- bootstrap a new service quickly
- generate common secrets such as database passwords or JWT secrets

## Syntax

```bash
envcraft generate --env <env> --preset <preset> [--preset <preset> ...]
```

## Presets

Current preset values:

- `postgres`
- `redis`
- `jwt`
- `stripe`
- `aws-s3`

## Canonical examples

Generate common local dev secrets:

```bash
envcraft generate --env dev --preset postgres --preset jwt
```

Generate plus one custom key:

```bash
envcraft generate \
  --env dev \
  --preset postgres \
  --preset jwt \
  --extra-key INTERNAL_API_TOKEN
```

Outside the repo:

```bash
envcraft generate \
  --env prod \
  --project my_app \
  --root /path/to/my_app \
  --preset jwt
```

## Writes

- remote secrets for the generated keys
- local `.envcraft.schema`
- synced control-plane schema

## Common mistakes

- expecting `generate` to reveal values; use it only to create and store them
