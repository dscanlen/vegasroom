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
vr ssh configure
vr ssh status
vr pi [workspace] [pi-args...]
vr shell [workspace]
```

Source-development equivalents:

```bash
cargo run -- init
cargo run -- init --build
cargo run -- doctor
cargo run -- ssh configure
cargo run -- ssh status
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

## Development checks

Before opening a PR or handing off changes, run:

```bash
bash scripts/check.sh
```

This performs:

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
```

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
- default image `vegasroom/pi:local` from `harness.pi.image`
- `docker --context rootless compose run --rm pi`
- ephemeral container removed after exit
- `/workspace` mounted from the resolved host workspace, defaulting to `~/.vegasroom/workspace`
- Pi state mounted from `~/.vegasroom/harness/pi/...`
- `~/.vegasroom/ssh` mounted once at `/home/agent/.ssh`; `/root/.ssh` is an image-level symlink to that path for root-run SSH/Git compatibility
- workspace mount can be made read-only with `harness.pi.read_only_workspace: true`
- container root filesystem can be made read-only with opt-in `harness.pi.read_only_rootfs: true`
- ssh-agent socket forwarded only when `$SSH_AUTH_SOCK` is usable, or through Vegasroom-managed SSH keys
- default `network_mode=host` from `harness.pi.network`
- default `build.network=host` from `harness.pi.build_network`
- non-host runtime network modes such as `bridge` are validation experiments until build, Git, internet, and Pi `/login` all work
- container runs as root inside rootless Docker for MVP bind-mount compatibility
- `no-new-privileges:true`, `cap_drop: ALL`, and `init: true` are enabled for low-risk runtime hardening


## Workspace model

`vr pi` and `vr shell` accept an optional workspace argument. Vegasroom resolves that host path and mounts it as `/workspace` inside the room.

```bash
vr pi
vr pi .
vr pi my-git-repo
vr pi ~/workspace/my-git-repo
vr pi /home/dan/workspace/my-git-repo

vr shell
vr shell .
vr shell my-git-repo
```

Resolution rules:

```text
no workspace     ~/.vegasroom/workspace
.                current host directory
name             ~/.vegasroom/workspace/name
relative/path    relative to current host directory
~/path           expanded against host home
/absolute/path   used directly if it exists
```

For `vr pi my-git-repo`, Vegasroom may create `~/.vegasroom/workspace/my-git-repo` if missing. External absolute paths must already exist. Credential directories such as `~/.ssh`, `~/.config`, `~/.aws`, `~/.gcloud`, and `~/.kube` are refused as workspaces. Vegasroom state outside the configured managed workspace root is also refused. Safe symlinked project directories are allowed with a warning; symlinks to blocked targets are refused. Set `workspace.risky_mount_policy: deny` to refuse broad warning-level mounts such as the host home directory or `/tmp`.

Set `harness.pi.read_only_workspace: true` in `~/.vegasroom/config.yaml` to mount `/workspace` read-only. This applies to the default workspace and to explicit command-line workspace arguments such as `vr pi .`, `vr pi my-git-repo`, and `vr pi /path/to/project`.

Set `harness.pi.read_only_rootfs: true` to make the container root filesystem read-only while keeping explicit Vegasroom mounts and tmpfs scratch paths writable.

Pi-specific arguments can be passed through after the workspace, after an explicit separator, or at top level when the first token is a flag other than Vegasroom help/version flags:

```bash
vr pi --session <id>
vr pi . --session <id>
vr pi my-git-repo --session <id>
vr pi . -- --help
vr --session <id>
vr -- ask Pi a question
```

`vr pi --help` shows Vegasroom's Pi wrapper help. Use `vr pi -- --help` to pass `--help` to Pi itself. Use `vr -- ...` when the first Pi argument is positional or ambiguous.

## SSH model

Vegasroom does not copy SSH private keys into the container and does not mount host `~/.ssh`.

Vegasroom supports two SSH paths:

- host-agent forwarding, when the host already has a usable `SSH_AUTH_SOCK`
- managed SSH, where `vr` starts a temporary `ssh-agent`, adds user-selected keys, forwards only that socket, then stops the agent when the room exits

Configure managed SSH keys interactively:

```bash
vr ssh configure
```

By default this recursively scans `~/.ssh`. To scan another root:

```bash
vr ssh configure /mnt/secrethost/.ssh
```

Symlinked directories are not followed by default. To opt in:

```bash
vr ssh configure --follow-symlinks ~/.ssh
```

Show the current SSH configuration:

```bash
vr ssh status
```

At launch, the container receives:

```bash
SSH_AUTH_SOCK=/run/vegasroom-ssh-agent.sock
```

This allows Git-over-SSH without copying private key files. It is still powerful: processes in the container can ask the forwarded agent to sign SSH authentication requests while the socket is mounted.

## Git identity model

SSH authentication and Git commit authorship are separate. Vegasroom injects a Git identity into the room when one can be resolved, so commits do not fall back to the container user.

Precedence:

```text
1. git.user_name and git.user_email in ~/.vegasroom/config.yaml
2. exactly one selected SSH key with git_user_name and git_user_email
3. host global Git config when git.inherit_host is true
```

The room receives Git identity through a per-launch generated Compose override and read-only generated gitconfig. Run `vr doctor` to see the effective identity.

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

## More documentation

- [Managed SSH](docs/managed-ssh.md)
- [Workspaces](docs/workspaces.md)
- [Security](docs/security.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Configuration](docs/config.md)

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

`vr doctor` also reports whether managed SSH keys are configured, whether key fingerprints still match, whether the room can receive an SSH agent socket, and which Git identity will be available inside the room.

### `vr ssh configure`

Recursively scans SSH key roots and lets you choose which keys Vegasroom should add to a temporary managed `ssh-agent` when launching a room.

```bash
vr ssh configure
vr ssh configure /mnt/secrethost/.ssh
vr ssh configure --follow-symlinks ~/.ssh
```

Selected rows are displayed with a tick and green text. Unselected rows use an empty box and the default terminal color. The selector uses a fixed-height key list and a wrapped details pane for the highlighted key, so long paths stay readable without corrupting the list layout. The TUI renders by absolute terminal coordinates rather than newline-driven output to avoid stepped-line behavior in raw terminal mode. Use arrow keys or `k`/`j` to move, Enter/Space to toggle, `s` to save without quitting, `q` to quit, and `r` to rescan. If there are unsaved changes, quitting prompts for `y` save-and-quit or `n` discard-and-quit.

### `vr ssh status`

Shows the configured SSH mode, selected keys, host agent status, and what Vegasroom will do on the next `vr pi` or `vr shell` launch.

```bash
vr ssh status
```

### `vr pi` and `vr`

Launch Pi interactively in the room:

```bash
vr pi
```

Launch Pi against a specific workspace:

```bash
vr pi .
vr pi my-git-repo
vr pi ~/workspace/my-git-repo
vr pi /home/dan/workspace/my-git-repo
```

Pass Pi-specific options:

```bash
vr pi --session <id>
vr pi . --session <id>
vr pi . -- --help
```

Equivalent default:

```bash
vr
vr --session <id>
vr -- ask Pi a question
```

### `vr shell`

Launches a shell in the same runtime:

```bash
vr shell
vr shell .
vr shell my-git-repo
```

Use this to inspect mounts, SSH agent forwarding, Git, network behavior, and Pi state paths.

## Security boundary

Vegasroom MVP reduces accidental broad host filesystem access by only mounting explicit directories, but it is not a hardened sandbox.

Known MVP tradeoffs:

- container runs as root inside rootless Docker, with `no-new-privileges:true` and `cap_drop: ALL`
- host networking is enabled
- workspace is mounted read-write by default unless `harness.pi.read_only_workspace` is enabled
- container root filesystem remains writable by default unless `harness.pi.read_only_rootfs` is enabled
- Pi state and auth are mounted read-write
- SSH agent forwarding can authorize SSH operations
- provider/API-key handling is deferred
- hardening is deferred

Read `docs/security.md` before evaluating isolation guarantees.

## Documentation

- `docs/design.md`
- `docs/rootless-docker.md`
- `docs/config.md`
- `docs/workspaces.md`
- `docs/security.md`
- `docs/troubleshooting.md`
- `docs/m5-mvp-notes.md`
