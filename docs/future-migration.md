# Future: Migration, Scaling, and Hybrid Secret Storage

This document describes a future EnvCraft capability.

It is **not implemented yet**.

The goal is to make it easier to migrate existing repositories that already store configuration in GitHub repository secrets, GitHub repository variables, or other ad-hoc flows, and to document how EnvCraft could scale beyond a single control-plane repository.

## Why this matters

Many existing projects already have:

- GitHub repository secrets
- GitHub repository variables
- CI-only environment values
- deployment values spread across multiple systems

EnvCraft already solves the new steady-state workflow.

What is still missing is a guided migration path from those existing setups into a clean EnvCraft project contract.

There is also a practical scaling limit in the current design:

- a single control-plane repository inherits GitHub repository secret limits
- this is acceptable for the current implementation and early projects
- it becomes tighter as many projects and environments accumulate in one place

## Proposed future capabilities

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

Additional future storage modes:

- keep a secret in the EnvCraft control-plane repository
- keep a secret directly in the target project repository
- keep a secret in a shared/global group managed by EnvCraft for cross-project reuse

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

## Future hybrid storage model

One useful future direction is to separate **management** from **physical storage**.

In that model, EnvCraft would remain the only management surface, but a value would not always need to live in the same repository.

### Mode 1: Control-plane managed

- secret value stored in `envcraft-secrets`
- current default model
- best fit for centralized reads such as `reveal`, `pull`, and `deploy-inject`

### Mode 2: Repo-managed

- secret value stored directly in the target application repository
- EnvCraft still owns the logical key, environment mapping, and metadata
- best fit for project-local CI values that do not need to be shared globally

### Mode 3: Shared/global

- secret value stored in EnvCraft under a shared group
- best fit for values reused across many repositories
- examples:
  - Play Store signing assets
  - shared CI credentials
  - service accounts reused across a family of apps

In all three cases, EnvCraft would still keep the project contract in the control-plane metadata.

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

The same rule would still apply in a future hybrid storage model.

## Advantages of the hybrid model

- scales better than storing every project secret in one control-plane repository
- lets CI use repo-local secrets where that is operationally simpler
- keeps shared assets centralized only when they are truly shared
- preserves one CLI and one schema-driven UX through EnvCraft
- allows `envcraft set --project ...` to keep working as the single write path even if the physical storage backend differs

## Tradeoffs and design pressure

- it weakens the simplicity of a single physical source of truth
- it introduces multiple storage locations
- it increases drift risk if humans edit secrets directly in GitHub outside EnvCraft
- `reveal`, `pull`, and `list` would need to report where a value came from
- some read flows would need a future `source` model or source metadata in command output

The most important design rule would be:

- EnvCraft remains the only management surface
- storage backends may differ, but schema and routing stay centralized

## Recommended future scope

Future migration support should likely cover:

- GitHub repository variables
- GitHub repository secrets
- migration report generation
- optional dry-run mode
- explicit cleanup confirmation
- hybrid storage modes:
  - control-plane managed
  - repo-managed
  - shared/global

It should **not** try to auto-rewrite every deployment workflow on day one.

## Status

Current status:

- documented as future work
- not implemented
- intentionally deferred so the initial EnvCraft flow can be validated first
