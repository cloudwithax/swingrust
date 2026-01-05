# SwingMusic (Rust)

Self-hosted music server built with Rust and actix-web.

## Requirements

- Rust 1.85+ (this repo depends on crates that require a Cargo version with edition 2024 support)
- Docker (optional, for container deployment)

## Local development

Build:

```powershell
cargo build
```

Run (debug build):

```powershell
cargo run -- --host 0.0.0.0 --port 1970 --debug
```

Run (release build):

```powershell
cargo build --release
.\target\release\swingmusic.exe --host 0.0.0.0 --port 1970
```

Notes:

- First start may prompt for interactive setup if no users exist and you do not pass `--setup-config`.
- By default, the server stores data in a config directory near the executable (or an OS-appropriate config directory). You can control this with `--config`.

## Configuration and data location

The `--config` flag points at the parent directory used for SwingMusic data. The server creates a SwingMusic-specific subdirectory inside it and writes:

- `settings.json`
- `swingmusic.db`
- `userdata.db`
- `images/`, `backups/`, `plugins/`, `client/`

On Linux, if `--config` is your home directory, the subdirectory name is `.swingmusic`. Otherwise it is `swingmusic`.

## Unattended setup

To skip interactive prompts on first run, provide a JSON file via `--setup-config`.

The setup file supports:

- Any `UserConfig` fields (camelCase) from `src/config/user_config.rs`
- Optional `admin_username` / `admin_password` fields (snake_case) to create an admin user

Minimal `setup-config.json`:

```json
{
  "rootDirs": ["D:/Music"],
  "enableWatchdog": false,
  "admin_username": "admin",
  "admin_password": "r3A5m4LqP2v9W7k1H6n8"
}
```

Keep this file private (it contains credentials).

Run with unattended setup:

```powershell
cargo run --release -- --host 0.0.0.0 --port 1970 --setup-config .\setup-config.json
```

If `admin_username` / `admin_password` are omitted, the setup file is still applied but user creation falls back to the normal behavior (interactive setup when no users exist).

## Docker deployment

Build:

```powershell
docker build -t swingmusic:local .
```

Run with a persistent data volume:

```powershell
docker run --rm -it `
  -p 1970:1970 `
  -v swingmusic-data:/data `
  swingmusic:local
```

Notes:

- The container runs as a non-root user and writes config and databases under `/data` (the image sets `HOME=/data` and runs with `--config /data`).
- If you start without `--setup-config` and no users exist yet, the container needs an interactive TTY (`-it`) for first-run prompts.
- After starting, open http://localhost:1970

### Docker unattended setup

Create a local `setup-config.json` and run:

```powershell
docker run --rm `
  -p 1970:1970 `
  -v swingmusic-data:/data `
  -v "${PWD}/setup-config.json:/setup-config.json:ro" `
  swingmusic:local --setup-config /setup-config.json
```

### Mounting your music library in Docker

If your music is on the host, mount it into the container and reference the container path in `rootDirs` in your setup config.

Windows host path `D:\Music` mounted into container at `/music`:

```powershell
docker run --rm `
  -p 1970:1970 `
  -v swingmusic-data:/data `
  -v "D:/Music:/music:ro" `
  -v "${PWD}/setup-config.json:/setup-config.json:ro" `
  swingmusic:local --setup-config /setup-config.json
```

In `setup-config.json`:

```json
{
  "rootDirs": ["/music"],
  "admin_username": "admin",
  "admin_password": "password"
}
```

## Useful commands

Show server CLI flags:

```powershell
docker run --rm swingmusic:local --help
```

Reset a user password (interactive):

```powershell
docker run --rm -it -v swingmusic-data:/data swingmusic:local --password-reset
```
