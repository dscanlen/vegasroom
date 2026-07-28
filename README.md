# vegasroom

Vegasroom is a small CLI for running the Pi coding agent harness inside an ephemeral Docker container. It gives Pi a predictable room to work in: a mounted workspace at `/workspace`, persistent Pi state, optional SSH/Git access, and a disposable container that is removed when the session ends.

Vegasroom is a convenience wrapper around the default Pi harness, not a hardened sandbox.

## Requirements

- Linux
- Docker with Compose v2 (`docker compose`)
- A Docker context named `rootless`
- Rust and Cargo to build/install Vegasroom from this repository

Check the Docker context before installing:

```bash
docker context ls
docker --context rootless info
```

## Install

From the repository checkout:

```bash
cargo install --path .
```

This installs the `vr` command.

Initialize Vegasroom state and build the default Pi harness image:

```bash
vr init --build
```

`vr init --build` creates `~/.vegasroom`, writes the managed Docker Compose runtime, and builds `vegasroom/pi:latest` from the bundled Pi harness Dockerfile.

Verify the setup:

```bash
vr doctor
```

## Use

Launch Pi in the default managed workspace:

```bash
vr
```

Launch Pi in the current directory:

```bash
vr pi .
```

Launch Pi in a named managed workspace under `~/.vegasroom/workspace`:

```bash
vr pi my-project
```

Pass arguments through to Pi:

```bash
vr --session abc123
vr pi . --session abc123
vr -- ask Pi a question
```

Open a shell in the same container environment:

```bash
vr shell .
```

Configure Vegasroom interactively:

```bash
vr config
```

Use command help for the exact current command surface:

```bash
vr --help
vr pi --help
vr shell --help
```

## How it works

- `vr` defaults to `vr pi`.
- Each Pi or shell launch starts a fresh container through Docker Compose and removes it after exit.
- The selected host workspace is mounted at `/workspace`.
- Pi configuration, extensions, skills, sessions, npm-global installs, SSH state, caches, and selected tool state persist under `~/.vegasroom`.
- The default image runs `@earendil-works/pi-coding-agent` and includes common command-line tools such as Git, OpenSSH, ripgrep, and fd.
- Host SSH agent forwarding is used when available; managed SSH can be configured with `vr config`.
- Environment packages and toolchains can be configured with `vr config`; rebuild the image with `vr init --build` after changing image-level environment settings.

## Workspaces

Workspace arguments are resolved as follows:

```text
no workspace     ~/.vegasroom/workspace
.                current host directory
name             ~/.vegasroom/workspace/name
relative/path    relative to current host directory
~/path           expanded against host home
/absolute/path   used directly if it exists
```

Examples:

```bash
vr pi
vr pi .
vr pi ~/code/my-project
vr pi /home/me/code/my-project
```

## Development

Run the project checks before handing off changes:

```bash
bash scripts/check.sh
```
