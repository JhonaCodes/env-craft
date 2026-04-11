# `envcraft upgrade`

## Purpose

Download a published EnvCraft release and replace the current local binary.

## When to use it

- update to the latest release
- pin to a known release

## Syntax

```bash
envcraft upgrade [--version <tag>]
```

## Canonical examples

Upgrade to the latest release:

```bash
envcraft upgrade
```

Upgrade to a specific release:

```bash
envcraft upgrade --version v0.1.7
```

## Expected side effects

- downloads the release archive for the current platform
- replaces the current local binary

## Common mistakes

- expecting `upgrade` to update the control-plane repo; use [init](init.md) for that
