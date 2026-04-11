# `envcraft reveal`

## Purpose

Reveal one logical variable through the GitHub Actions delivery flow.

## When to use it

- inspect one value during debugging
- verify that one environment is configured correctly
- fetch one key without assembling a full `.env` file

## Syntax

```bash
envcraft reveal <LOGICAL_KEY> --env <env>
```

## Reads

- local `.envcraft.schema`
- GitHub Actions delivery artifact

## Writes

- nothing by default
- or a file if `--output` is provided

## Canonical examples

Print one value:

```bash
envcraft reveal API_BASE_URL --env prod
```

Write one value to a file:

```bash
envcraft reveal API_BASE_URL --env dev --output /tmp/api_base_url.env
```

Run outside the repo:

```bash
envcraft reveal API_BASE_URL \
  --env prod \
  --project my_app \
  --root /path/to/my_app
```

## Expected behavior

- dispatches the delivery workflow
- waits for GitHub Actions
- prints a delivery spinner while waiting
- prints the value or writes it to the target file

## Common mistakes

- using `reveal` when you actually want a full `.env`; use [pull](pull.md)
