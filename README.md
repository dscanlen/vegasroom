# Vegasroom

Vegasroom is a source-built MVP for running the Pi Agent Harness inside an ephemeral rootless Docker container.

The goal is practical containment for agent work: Pi runs in a container, only explicit state mounts persist, Git-over-SSH works through the host ssh-agent, and Pi login state persists through the Pi harness mount.

This MVP is functional, not hardened.

## Current status

Implemented commands:

```bash
vr
vr init
vr init --build
vr doctor
vr pi
vr shell
```

Source-development equivalents:

```bash
cargo run -- init
cargo run -- init --build
cargo run -- doctor
cargo run -- pi
cargo run -- shell
cargo run
```

`vr` defaults to `vr pi`.

## Requirements

MVP target:

- Linux
- Docker
- Docker Compose v2 through `docker compose`
- Docker context named `rootless`
- Rust toolchain for source builds

Check Docker contexts:

```bash
docker context ls
docker --context rootless info
```

## Quick start from source

Build and install the host command:

```bash
cargo build --release
cargo install --path .
```

Initialize state, write the managed runtime files, and build the local Pi image:

```bash
vr init --build
```

`vr init` writes the Compose file and Pi Dockerfile that were embedded into the installed binary to `~/.vegasroom/runtime/`. After installation, `vr` commands can be run from any directory and do not require the original git checkout to remain on disk.

Check readiness:

```bash
vr doctor
```

Launch Pi:

```bash
vr
```

Open the debug shell:

```bash
vr shell
```

If you do not want to install `vr`, use `cargo run -- ...` from the repo.

## State directory

Vegasroom uses:

```text
~/.vegasroom
```

Default layout:

```text
~/.vegasroom/
  config.yaml
  workspace/
  harness/pi/config/
  harness/pi/extensions/
  harness/pi/skills/
  harness/pi/sessions/
  ssh/
    known_hosts
  cache/
  runtime/
    compose.yaml
    harness/pi/Dockerfile
```

## Runtime model

The runtime is intentionally the proven M1-M4 model:

- Docker Compose service `pi` materialized under `~/.vegasroom/runtime`
- image `vegasroom/pi:local`
- `docker --context rootless compose run --rm pi`
- ephemeral container removed after exit
- `/workspace` mounted from `~/.vegasroom/workspace`
- Pi state mounted from `~/.vegasroom/harness/pi/...`
- `~/.vegasroom/ssh` mounted as the container SSH directory
- ssh-agent socket forwarded only when `$SSH_AUTH_SOCK` is usable
- `network_mode=host`
- `build.network=host`
- container runs as root inside rootless Docker for MVP bind-mount compatibility

## SSH model

Vegasroom does not copy SSH private keys into the container and does not mount host `~/.ssh`.

When the host has a usable ssh-agent socket, `vr pi` and `vr shell` generate a temporary Compose override that mounts that socket into the container at:

```text
/tmp/vegasroom/ssh-agent.sock
```

The container receives:

```bash
SSH_AUTH_SOCK=/tmp/vegasroom/ssh-agent.sock
```

This allows Git-over-SSH without copying private key files. It is still powerful: processes in the container can ask the host agent to sign SSH authentication requests while the socket is mounted.

## Pi login model

Pi login is handled by Pi itself through interactive `/login`.

The container sets:

```yaml
BROWSER: echo
```

so browser login helpers print a URL instead of attempting to launch a browser inside the container. Open that URL on the host.

Pi auth state is expected to persist under:

```text
~/.vegasroom/harness/pi/config/auth.json
```

Do not store provider API keys in `~/.vegasroom/config.yaml`; provider/API-key handling is out of scope for this MVP.

## Commands

### `vr init`

Creates or repairs the Vegasroom state directory. It does not delete existing files and does not overwrite an existing config file.

```bash
vr init
```

Build the local Pi image:

```bash
vr init --build
```

### `vr doctor`

Prints readiness checks using:

```text
PASS
WARN
FAIL
```

`WARN` means usable but degraded. `FAIL` means required functionality is missing.

### `vr pi` and `vr`

Launch Pi interactively in the room:

```bash
vr pi
```

Equivalent default:

```bash
vr
```

### `vr shell`

Launches a shell in the same runtime:

```bash
vr shell
```

Use this to inspect mounts, SSH agent forwarding, Git, network behavior, and Pi state paths.

## Security boundary

Vegasroom MVP reduces accidental broad host filesystem access by only mounting explicit directories, but it is not a hardened sandbox.

Known MVP tradeoffs:

- container runs as root inside rootless Docker
- host networking is enabled
- workspace is mounted read-write
- Pi state and auth are mounted read-write
- SSH agent forwarding can authorize SSH operations
- provider/API-key handling is deferred
- hardening is deferred

Read `docs/security.md` before evaluating isolation guarantees.

## Documentation

- `docs/design.md`
- `docs/rootless-docker.md`
- `docs/config.md`
- `docs/security.md`
- `docs/troubleshooting.md`
- `docs/m5-mvp-notes.md`
