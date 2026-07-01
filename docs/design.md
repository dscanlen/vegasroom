# Vegas Rooms Design Brief

## Project definition

Vegas Rooms is a Rust CLI for launching AI agent harnesses inside ephemeral rootless Docker containers.

Repository/package name:

```text
vegasroom
```

CLI command:

```bash
vr
```

User-facing name:

```text
Vegas Rooms
```

Default harness:

```text
Pi Agent Harness
```

Running:

```bash
vr
```

should behave like:

```bash
vr pi
```

## Product intent

Vegas Rooms creates a clean operating boundary between AI agent harnesses and the host system.

The harness should be able to:

```text
work on user-approved repositories in a shared workspace
use Git over SSH through the host ssh-agent
retain its own config and session state
run inside a fresh container each launch
```

The harness should not receive broad access to the host filesystem or direct copies of SSH private keys.

## MVP philosophy

The MVP prioritizes function first.

The first objective is not maximum security. The first objective is a usable workflow:

```text
Pi runs in a fresh rootless Docker container.
The user can work in /workspace.
Pi config and sessions persist.
Git over SSH works through ssh-agent forwarding.
Pi login works using host browser or headless flow.
```

Hardening comes later, after the working path is proven.

## Implementation language

Vegas Rooms is implemented in Rust.

The CLI should favor:

```text
clear code
small modules
readable errors
minimal abstraction
inspectable Docker behavior
good learning value
```

Recommended initial crates:

```text
clap
serde
serde_yaml
anyhow
thiserror
directories
```

## Runtime assumptions

MVP platform:

```text
Linux
Docker
rootless Docker context
```

Expected command shape:

```bash
docker --context rootless compose run --rm pi
```

Docker Compose is preferred because it keeps mounts, environment variables, network settings, and command overrides visible.

## Container lifecycle

Containers are ephemeral.

Every harness launch should create a new container and remove it after exit.

Persistent data must come only from explicit mounts.

Vegas Rooms should not depend on long-lived named containers for MVP.

## Directory model

Default state root:

```text
~/.vegas-rooms
```

Default layout:

```text
~/.vegas-rooms/
  harness/
    pi/
      config/
      extensions/
      skills/
      sessions/
  workspace/
  ssh/
    known_hosts
  cache/
  config.yaml
```

The repo name is `vegasroom`, but the local state directory remains `~/.vegas-rooms` for readability.

## Workspace model

Default host path:

```text
~/.vegas-rooms/workspace
```

Default container path:

```text
/workspace
```

The workspace is shared across harnesses.

For MVP, the workspace is mounted read-write.

Future work may support:

```bash
vr pi .
```

to dynamically mount the current directory as the workspace.

## Harness model

The MVP is config-driven but not plugin-driven.

Pi is the only active harness.

Claude may appear as commented future config, but `vr claude` is not part of MVP.

Example config shape:

```yaml
default_harness: pi

paths:
  root: ~/.vegas-rooms
  workspace: ~/.vegas-rooms/workspace

docker:
  context: rootless

harness:
  pi:
    enabled: true
    image: vegas-rooms/pi:local
    command: pi
    ssh_agent: auto
    network: default

  # claude:
  #   enabled: false
  #   image: vegas-rooms/claude:local
  #   command: claude
  #   ssh_agent: auto
  #   network: default
```

## SSH agent model

Vegas Rooms should use the host ssh-agent socket when available.

Host source:

```text
$SSH_AUTH_SOCK
```

Container target:

```text
/tmp/vegas-rooms/ssh-agent.sock
```

Container environment:

```bash
SSH_AUTH_SOCK=/tmp/vegas-rooms/ssh-agent.sock
```

If no SSH agent is available, Vegas Rooms should warn and continue.

The warning should be clear because Pi can still run without SSH.

## Known hosts model

Vegas Rooms should maintain its own known hosts file:

```text
~/.vegas-rooms/ssh/known_hosts
```

Container target:

```text
/home/agent/.ssh/known_hosts
```

Vegas Rooms should not mount the host’s full `~/.ssh` directory.

## Network model

MVP networking is open and functional.

Default:

```text
Docker bridge networking
```

The container should have outbound internet access.

Host service access should be supported where possible with:

```yaml
extra_hosts:
  - "host.docker.internal:host-gateway"
```

If Pi login requires callback ports, Vegas Rooms may reserve:

```text
14500-14599
```

Host networking is allowed only as a fallback if normal bridge networking cannot support Pi login.

## Login model

MVP login support focuses on Pi subscription login.

Preferred flows:

```text
browser login using host browser
headless login
```

API key management is not part of MVP.

Auth state may persist in the Pi config mount for MVP.

This must be revisited during hardening.

## Security boundary

The MVP security goal is:

```text
Prevent accidental host filesystem damage.
```

Vegas Rooms MVP does not claim to protect against malicious agents, credential misuse, network exfiltration, or container escapes.

The MVP should avoid dangerous mounts.

Do not mount:

```text
/
 /home
host Docker socket
full ~/.ssh
browser profiles
cloud credential directories
arbitrary host directories
```

Allowed MVP mounts:

```text
workspace
Pi harness config/state
Vegas-managed known_hosts
host ssh-agent socket
generated Vegas Rooms cache/config as needed
```

## First-run disclaimer

The first time the user runs `vr`, show:

```text
Vegas Rooms launches AI agent harnesses inside ephemeral rootless Docker containers.

Only configured mounts persist. Your workspace and harness config are mounted read-write.

Your SSH private keys are not copied into the container, but the forwarded ssh-agent socket can authorize SSH operations while mounted.

Default harness: Pi. Other harnesses can be added in future versions.
```

The disclaimer should be shown on first run only.

## Development framework

Vegas Rooms uses:

```text
Walking Skeleton + Capability Milestones
```

Each milestone proves one concrete capability.

Risky work moves through:

```text
Spike      — prove it manually
Stabilize  — turn it into project structure or Rust code
Ship       — document and make it usable
```

The rule:

```text
Every milestone must make Vegas Rooms more runnable.
```

## Lean milestone map

### M0 — Shape

Define the project, MVP, boundaries, naming, and next handover.

### M1 — Room

Prove that Pi can run inside an ephemeral rootless Docker container.

### M2 — Command

Wrap the working container flow with the Rust `vr` CLI.

### M3 — Keys

Make Git-over-SSH work through forwarded ssh-agent without copying private keys.

### M4 — Login

Make Pi subscription login work from inside the container.

### M5 — MVP

Make the project coherent and usable from source.

## Later bucket

Post-MVP work:

```text
security profiles
read-only workspace mode
dangerous mount warnings
network restrictions
deploy-key guidance
vr pi .
vr claude
Podman
macOS
WSL2
registry images
installers
```
