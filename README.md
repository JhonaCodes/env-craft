# EnvCraft

EnvCraft is a Rust 2024 CLI for managing environment variables across many projects while keeping **GitHub Secrets** as the only secret store.

V1 goals in this repository:
- bootstrap a central control-plane repo managed by EnvCraft
- keep `.envcraft.schema` as the contract for each project
- create and update secrets in GitHub
- reveal one secret at a time through **GitHub Actions**
- build local `.env` files and deploy-time export scripts from per-key delivery

## Current V1 transport

This first implementation uses:
- GitHub Secrets as storage
- GitHub Actions as the only authorized secret reader
- encrypted one-time payloads returned as workflow artifacts

That keeps the system usable without additional public socket infrastructure while preserving the core rule: secret values are only read inside Actions.

## Main commands

```bash
envcraft init \
  --github-owner JhonaCodes \
  --control-repo envcraft-secrets \
  --bootstrap-dir /path/to/envcraft-secrets

envcraft link --project nui-app --env dev --env prod

envcraft set DB_PASSWORD --env prod --generate

envcraft generate --env dev --preset postgres --preset jwt

envcraft list --remote

envcraft reveal DB_PASSWORD --env prod

envcraft pull --env dev --output .env.dev

envcraft deploy-inject --env prod > env.sh
```

## Control-plane bootstrap

`envcraft init --bootstrap-dir ...` writes:
- `.github/workflows/deliver.yml`
- `.github/scripts/envcraft-deliver.mjs`
- `projects/.gitkeep`

The generated workflow handles **single-key delivery**. `pull` and `deploy-inject` assemble the final payload by requesting each declared key independently.

## Environment contract

Each application repo uses `.envcraft.schema`:

```yaml
project: nui-app
environments:
  - dev
  - prod
vars:
  DB_PASSWORD:
    vault_key: NUI_APP_PROD_DB_PASSWORD
    type: secret
    generate: true
    required: true
```

## Notes

- GitHub does not expose secret values through its API.
- Because of that, EnvCraft writes secrets directly through the GitHub Secrets API, but reads them only through Actions.
- Full `.env` delivery currently costs one workflow run per key. That is acceptable for V1 correctness and can be optimized later.
