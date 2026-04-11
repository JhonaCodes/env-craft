# Context Resolution

EnvCraft tries to infer the active project from the current directory.

## Default behavior

If you run a command inside an application repo:

- EnvCraft reads `.envcraft.schema`
- It uses that file to determine the active project
- You usually only need `--env`

Example:

```bash
cd /path/to/acordio_app
envcraft reveal API_BASE_URL --env dev
```

## `--root`

Use `--root` when you are not currently inside the application repo but still want EnvCraft to read that repo's `.envcraft.schema`.

Example:

```bash
envcraft reveal API_BASE_URL \
  --env dev \
  --root /path/to/acordio_app
```

## `--project`

Use `--project` when:

- you want to override the project slug
- there is no local `.envcraft.schema` yet
- you are recovering from bad local context

Example:

```bash
envcraft set API_BASE_URL \
  --env dev \
  --project acordio_app \
  --root /path/to/acordio_app \
  --value https://api-dev.acordio.app
```

## Which commands use context

Commands that usually rely on `.envcraft.schema`:

- `link`
- `set`
- `generate`
- `list`
- `reveal`
- `pull`
- `deploy-inject`

Commands that do not depend on project context:

- `init`
- `upgrade`

## State changes

Commands that mutate state:

- `init`
- `link`
- `set`
- `generate`

Commands that only read or deliver:

- `list`
- `reveal`
- `pull`
- `deploy-inject`
- `upgrade`

