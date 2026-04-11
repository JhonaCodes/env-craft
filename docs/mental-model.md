# Mental Model

EnvCraft has four places that matter:

1. Your application repository
   - Contains `.envcraft.schema`
   - Defines the logical contract for variables such as `API_BASE_URL`

2. The control-plane repository
   - Usually `envcraft-secrets`
   - Contains the GitHub Actions workflow and a synced copy of each project schema under `projects/<project>/.envcraft.schema`

3. GitHub Secrets
   - Stores the actual values
   - EnvCraft writes values here directly
   - EnvCraft reads values only through GitHub Actions delivery

4. Your local machine
   - Stores global EnvCraft config in `~/.envcraft/config.toml`
   - Stores a local clone of the control-plane repo in `~/.envcraft/repos/<control-repo>`

## Authentication model

EnvCraft can run in two broad contexts:

1. Interactive local or remote shell
   - Example: your laptop, a VPS, a Dokploy host, or a server shell
   - EnvCraft can use `GITHUB_TOKEN` if present
   - Or it can fall back to the local `gh` session automatically

2. Non-interactive CI such as GitHub Actions
   - There is no interactive `gh` session to reuse
   - Prefer GitHub App credentials through `ENVCRAFT_GITHUB_APP_ID` plus `ENVCRAFT_GITHUB_APP_PRIVATE_KEY` or `ENVCRAFT_GITHUB_APP_PRIVATE_KEY_FILE`
   - EnvCraft can still use an explicit token through `GITHUB_TOKEN` as a legacy fallback
   - If the workflow needs to read from a separate private control-plane repo, the repository's default `GITHUB_TOKEN` is usually not enough
   - In that case, install a GitHub App with access to the control-plane repo and provide its credentials to the workflow

## When dedicated CI auth is needed

You only need dedicated non-interactive auth in repositories or workflows that run EnvCraft inside GitHub Actions and need access to a separate private control-plane repo such as `envcraft-secrets`.

You do not need it for:

- normal local development on your machine
- a remote server that already has its own `gh` session or an explicit `GITHUB_TOKEN`
- repos that do not run EnvCraft inside Actions

## Default flow

1. `envcraft init`
   - Bootstrap or sync the control-plane repo

2. `envcraft link`
   - Create `.envcraft.schema` inside the app repo

3. `envcraft set`
   - Create or update one secret value for one environment
   - Update local schema
   - Sync project schema into the control-plane repo

4. `envcraft reveal` or `envcraft pull`
   - Trigger GitHub Actions
   - Receive an encrypted payload
   - Print one value or assemble a `.env` file

## Secret naming

EnvCraft stores secrets in GitHub using deterministic names:

- `<PROJECT>_<ENV>_<KEY>`
- Example: `ACORDIO_APP_PROD_API_BASE_URL`

Inside your app repo, you still work with logical keys:

- `API_BASE_URL`
- `LEGAL_BASE_URL`
