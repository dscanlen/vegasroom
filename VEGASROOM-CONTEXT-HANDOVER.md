# Vegasroom Contextual Handover

## Purpose

This handover is for an agent starting from scratch with access to the current MVP git repository.

The project is called:

```text
vegasroom
```

The user-facing product name is:

```text
Vegasroom
```

The CLI command is:

```bash
vr
```

The default harness is:

```text
Pi
```

The canonical state directory is:

```text
~/.vegasroom
```

Do not revert to the older spelling:

```text
~/.vegas-rooms
```

That older spelling appears in earlier handover material, but the current MVP has been aligned to `~/.vegasroom`.

---

# Current MVP status

Vegasroom is a working MVP.

The MVP proves:

```text
Pi can run inside an ephemeral rootless Docker container.
The Rust CLI wraps Docker Compose successfully.
The host state directory is created and repaired by vr init.
Readiness checks are exposed through vr doctor.
Pi launches through vr pi and vr.
A debug shell launches through vr shell.
SSH agent forwarding works when the host has a usable agent.
Git-over-SSH works inside the room without copying private keys.
Pi subscription login works and persists across ephemeral container launches.
```

The current source workflow should work:

```bash
cargo build --release
cargo install --path .
vr init --build
vr doctor
vr
```

Development workflow should also work:

```bash
cargo run -- init
cargo run -- doctor
cargo run -- shell
cargo run -- pi
```

---

# Repository shape

Expected repo shape:

```text
vegasroom/
  Cargo.toml
  README.md
  compose.yaml
  docs/
    design.md
    rootless-docker.md
    security.md
    config.md
    troubleshooting.md
    m*-*.md
  harness/
    pi/
      Dockerfile
  src/
    main.rs
    cli.rs
    config.rs
    docker.rs
    doctor.rs
    paths.rs
    ssh.rs
```

The exact docs may include milestone notes from M1–M5. Do not reorganize purely for aesthetics.

---

# Runtime model

Vegasroom uses Docker Compose as the runtime mechanism.

The key command shape is:

```bash
docker --context rootless compose run --rm pi
```

The Rust wrapper invokes this shape internally.

Current Docker/Compose assumptions:

```text
Linux host
Docker installed
Docker Compose available through docker compose
Docker context named rootless
rootless Docker context usable
local image vegasroom/pi:local
```

Current Compose model preserves:

```text
build.network=host
network_mode=host
container-root runtime
interactive TTY
ephemeral container removal through compose run --rm
workspace mounted at /workspace
Pi state mounted explicitly
SSH directory mounted explicitly
SSH agent socket forwarded only when available
```

Do not redesign this runtime unless a specific defect requires it.

---

# State model

Canonical host state root:

```text
~/.vegasroom
```

Expected state layout:

```text
~/.vegasroom/
  config.yaml
  workspace/
  harness/
    pi/
      config/
      extensions/
      skills/
      sessions/
  ssh/
    known_hosts
  cache/
```

Important mappings:

```text
~/.vegasroom/workspace
  -> /workspace

~/.vegasroom/harness/pi/config
  -> /home/agent/.pi/agent

~/.vegasroom/harness/pi/extensions
  -> /home/agent/.pi/extensions

~/.vegasroom/harness/pi/skills
  -> /home/agent/.pi/skills

~/.vegasroom/harness/pi/sessions
  -> /home/agent/.pi/sessions

~/.vegasroom/ssh
  -> /home/agent/.ssh
  -> /root/.ssh

~/.vegasroom/cache
  -> /home/agent/.cache
```

The container currently runs as root, but `HOME` is set to:

```text
/home/agent
```

Pi auth state is expected at:

```text
/home/agent/.pi/agent/auth.json
```

which persists on the host at:

```text
~/.vegasroom/harness/pi/config/auth.json
```

Do not mount the host `~/.ssh`.

Do not copy private SSH keys into the container.

---

# CLI behavior

## `vr`

Default command.

Equivalent to:

```bash
vr pi
```

Expected behavior:

```text
show first-run disclaimer once
create/repair safe state
load config
launch Pi through Docker Compose
return Docker/Compose exit code where practical
```

## `vr init`

Creates or repairs state:

```bash
vr init
```

Builds the local Pi image as well:

```bash
vr init --build
```

Expected behavior:

```text
create missing directories
create config.yaml if missing
create ~/.vegasroom/ssh as a directory
create known_hosts if current implementation does so
do not delete existing files
do not overwrite user config silently
fail clearly if an expected directory path exists as a file
```

## `vr doctor`

Readiness report:

```bash
vr doctor
```

Output format:

```text
PASS
WARN
FAIL
```

Rules:

```text
PASS means ready.
WARN means usable but degraded.
FAIL means required functionality is missing.
```

Doctor should check:

```text
Docker binary
Docker Compose
Docker context
rootless context usability
trivial container run
compose.yaml
Pi Dockerfile
Pi image
state directories
config.yaml
SSH_AUTH_SOCK status
SSH agent socket usability
container can receive SSH_AUTH_SOCK when available
ssh-add inside container
network reachability
Pi config/session writability
Pi auth-state presence when practical
```

Missing SSH agent is a warning, not a failure.

Missing Pi auth before login is a warning, not a failure.

## `vr pi`

Launches Pi:

```bash
vr pi
```

Expected behavior:

```text
ensure state
load config
prepare SSH agent override if available
invoke Docker Compose
start Pi interactively
remove the container after exit
```

## `vr shell`

Launches the debug shell:

```bash
vr shell
```

Expected behavior:

```text
same Docker context
same Compose file
same state directories
same mount model
same build.network=host
same network_mode=host
same container-root runtime
same SSH_AUTH_SOCK behavior
```

This is the primary debugging command.

---

# Config model

Config path:

```text
~/.vegasroom/config.yaml
```

Default shape:

```yaml
default_harness: pi

paths:
  root: ~/.vegasroom
  workspace: ~/.vegasroom/workspace

docker:
  context: rootless
  compose_file: ./compose.yaml

harness:
  pi:
    enabled: true
    image: vegasroom/pi:local
    command: pi
    ssh_agent: auto
    network: host

  # claude:
  #   enabled: false
  #   image: vegasroom/claude:local
  #   command: claude
  #   ssh_agent: auto
  #   network: host
```

Current implementation uses only a small subset of config. Do not expand the config model unless the milestone requires it.

---

# SSH model

M3 proved SSH agent forwarding.

Current model:

```text
detect host SSH_AUTH_SOCK
verify it exists and is a socket
generate temporary Compose override if usable
forward socket to /tmp/vegasroom/ssh-agent.sock
set container SSH_AUTH_SOCK=/tmp/vegasroom/ssh-agent.sock
warn but continue when unavailable
```

Security properties:

```text
private keys are not copied into the container
host ~/.ssh is not mounted into the container
only the agent socket is forwarded
processes inside the container can request SSH signatures while the socket is mounted
this is useful but powerful
```

M3 proof commands inside `vr shell`:

```bash
echo "$SSH_AUTH_SOCK"
ssh-add -l
ssh -T git@github.com
git clone git@github.com:OWNER/REPO.git
git fetch
```

Do not add key generation or deploy-key automation unless a future milestone scopes it.

---

# Pi login model

M4 proved Pi login.

Current model:

```text
Pi runs inside the room.
User runs /login inside Pi.
Pi prints or exposes a login flow.
Host browser is used where possible.
BROWSER=echo is set to avoid trying to launch a browser inside the container.
Auth state persists through the Pi config mount.
```

Expected persisted auth path:

```text
~/.vegasroom/harness/pi/config/auth.json
```

Do not mount host browser profiles.

Do not store provider API keys in Vegasroom config.

Do not add provider-specific auth handling unless the milestone explicitly scopes it.

---

# Security posture

Be honest about the MVP security boundary.

Current MVP improves host hygiene by avoiding broad host mounts and avoiding private-key copies, but it is not a hardened sandbox.

Known tradeoffs:

```text
container runs as root
network_mode=host is used
build.network=host is used
workspace is mounted read-write
Pi state/auth is mounted read-write
SSH agent socket forwarding is powerful
Pi auth state persists in the Pi harness mount
provider/API-key handling is out of scope
hardening is deferred
```

Do not claim:

```text
complete isolation
strong sandboxing
credential isolation
network isolation
secret management
```

The correct framing is:

```text
Vegasroom is a functional MVP that runs agent harnesses inside ephemeral Docker containers with explicit persistent mounts.
```

---

# Important historical decisions

## Rootless Docker bridge failed

M1 encountered rootless Docker bridge/veth failures. The working model uses:

```text
build.network=host
network_mode=host
```

Do not switch back to bridge networking casually.

## Single-file known_hosts mount failed

Mounting:

```text
~/.vegasroom/ssh/known_hosts -> /home/agent/.ssh/known_hosts
```

caused file-vs-directory problems when Docker created missing bind sources as directories.

The current model mounts the whole SSH directory:

```text
~/.vegasroom/ssh -> container SSH directory
```

Preserve this unless a future milestone intentionally changes it.

## Non-root container user caused friction

An earlier attempt to use a non-root user conflicted with base image UID/GID behavior and bind-mount write behavior.

The current MVP intentionally uses:

```yaml
user: "0:0"
```

inside rootless Docker.

Do not treat this as an accidental bug. It is a documented MVP tradeoff.

## Pi helper tools

Pi wanted `fd` and `ripgrep`. The image installs them to avoid Pi downloading helpers into state at runtime.

Debian uses:

```text
fdfind
```

so the image symlinks:

```text
fd -> fdfind
```

---

# Current quality gate

The MVP should pass:

```bash
cargo fmt
cargo clippy
cargo build
cargo build --release
cargo install --path .
```

Then:

```bash
vr init --build
vr doctor
vr shell
vr pi
vr
```

Expected:

```text
vr is installed
state is created
image builds
doctor has no unexpected FAIL entries
shell opens
Pi opens
SSH works when host agent exists
Pi login persists after relaunch
vr defaults to vr pi
```

---

# Recommended next milestone

The best next milestone is:

```text
M6 — Workspace
```

Goal:

```text
Support vr pi <workspace> and vr shell <workspace>.
```

This should include:

```text
vr pi .
vr shell .
vr pi my-git-repo
vr shell my-git-repo
VR_WORKSPACE override for Compose
safe workspace path resolution
dangerous path warnings or failures
clear docs
```

Suggested following milestones:

```text
M7 — Managed SSH
M8 — Bootstrap
M9 — Harden
M10 — Extend
```

---

# Do not do next

Unless explicitly asked, do not jump into:

```text
Claude support
security hardening
non-root migration
network isolation
provider/API-key handling
installer packaging
registry image publishing
deploy-key automation
Git signing
Podman/macOS/WSL2 support
```

The next highest-value work is improving the user path around workspaces, SSH setup, and host bootstrap.
