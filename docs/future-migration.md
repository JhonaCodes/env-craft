# Future: Migration and Import Flows

This document describes a future EnvCraft capability.

It is **not implemented yet**.

The goal is to make it easier to migrate existing repositories that already store configuration in GitHub repository secrets, GitHub repository variables, or other ad-hoc flows.

## Why this matters

Many existing projects already have:

- GitHub repository secrets
- GitHub repository variables
- CI-only environment values
- deployment values spread across multiple systems

EnvCraft already solves the new steady-state workflow.

What is still missing is a guided migration path from those existing setups into a clean EnvCraft project contract.

## Proposed future capability

Potential command shape:

```bash
envcraft migrate github-repo --from my-org/my-app --project my_app
```

Possible modes:

- import GitHub repository variables directly
- import GitHub repository secrets through a temporary GitHub Actions migration workflow
- map imported values to `dev`, `staging`, or `prod`
- write them into the EnvCraft control plane
- update `.envcraft.schema`
- produce a migration report before cleanup

## Important technical constraint

GitHub repository **variables** can be read by API.

GitHub repository **secrets** cannot be read by API as plain values.

Because of that, a future migration flow would need two different mechanisms:

- Variables:
  - direct API import
- Secrets:
  - temporary GitHub Actions workflow in the source repository
  - read values inside Actions
  - deliver them to EnvCraft through a one-time encrypted session
  - store them in the EnvCraft control-plane repository

## Proposed migration phases

### 1. Discovery

EnvCraft would inspect the source repository and produce:

- repository variables found
- repository secrets found
- suggested EnvCraft logical key names
- possible environment mapping

### 2. Review

Before writing anything, EnvCraft should show:

- what will be imported
- what values are variables vs secrets
- which target environment each value will be stored under
- what local schema changes will be made

### 3. Import

EnvCraft would then:

- import variables directly
- import secrets through a temporary workflow
- write them into the control-plane repository
- sync `.envcraft.schema`

### 4. Validation

After import, EnvCraft should help validate:

- `envcraft list --remote`
- `envcraft reveal <KEY> --env <env>`
- `envcraft pull --env <env>`
- `envcraft deploy-inject --env <env>`

### 5. Cleanup

Only after successful validation should the old values be removed from the source repository.

Cleanup should remain an explicit action, not an automatic one.

## What should remain manual

Even with a future migration command, deployment wiring should still remain mostly manual.

Reason:

- GitHub Actions build pipelines vary by project
- remote servers vary by project
- Dokploy usage varies by project
- some apps need `pull`
- some apps need `deploy-inject`

EnvCraft can guide this, but should not guess the deployment integration automatically.

## Recommended future scope

Future migration support should likely cover:

- GitHub repository variables
- GitHub repository secrets
- migration report generation
- optional dry-run mode
- explicit cleanup confirmation

It should **not** try to auto-rewrite every deployment workflow on day one.

## Status

Current status:

- documented as future work
- not implemented
- intentionally deferred so the initial EnvCraft flow can be validated first
