# Vegas Rooms

Vegas Rooms is a minimalist CLI for launching AI agent harnesses inside ephemeral rootless Docker containers.

Repository/package name:

```text
vegasroom
```

Command:

```bash
vr
```

Default harness:

```text
Pi Agent Harness
```

Running:

```bash
vr
```

is equivalent to:

```bash
vr pi
```

## Purpose

Vegas Rooms lets a user run AI coding harnesses against a shared workspace without giving the harness broad access to the host filesystem or copying SSH credentials into the container.

The MVP is focused on practical isolation, not hardened sandboxing.

The first goal is simple:

```text
Run Pi Agent Harness inside a fresh rootless Docker container,
mount a shared workspace,
forward the host ssh-agent socket,
and preserve only explicit harness state.
```

## MVP scope

The MVP supports:

```bash
vr
vr init
vr doctor
vr pi
vr shell
```

### `vr`

Launches the default harness.

For MVP, this means Pi.

### `vr init`

Creates the local Vegas Rooms state directory.

Default location:

```text
~/.vegas-rooms
```

Expected structure:

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

### `vr doctor`

Checks whether the host is ready to run Vegas Rooms.

It should check:

```text
Docker availability
rootless Docker context
basic container execution
Vegas Rooms directory structure
workspace path
Pi harness path
SSH_AUTH_SOCK availability
known_hosts file
basic network access
```

### `vr pi`

Launches Pi Agent Harness inside a fresh ephemeral container.

The container should be removed after exit.

### `vr shell`

Launches the same container environment as `vr pi`, but opens a shell instead of Pi.

This is used for debugging filesystem, SSH, Git, login, and network behavior.

## Runtime model

Vegas Rooms uses rootless Docker on Linux for the MVP.

Expected runtime shape:

```bash
docker --context rootless compose run --rm pi
```

Each invocation creates a fresh container.

Persistent data comes only from explicit mounts.

## Default mounts

The MVP should mount:

```text
~/.vegas-rooms/workspace              -> /workspace
~/.vegas-rooms/harness/pi/...         -> Pi config/state paths
~/.vegas-rooms/ssh/known_hosts        -> /home/agent/.ssh/known_hosts
$SSH_AUTH_SOCK                        -> /tmp/vegas-rooms/ssh-agent.sock
```

Inside the container:

```bash
SSH_AUTH_SOCK=/tmp/vegas-rooms/ssh-agent.sock
```

## SSH model

Vegas Rooms should not copy SSH private keys into the container.

Instead, it forwards the host ssh-agent socket.

This allows Git over SSH to work without mounting private keys.

Important limitation:

```text
Forwarding the ssh-agent socket means processes inside the container can request SSH signatures from the host agent while the socket is mounted.
```

Private keys are not copied, but the mounted agent socket is still powerful.

## Security boundary

The MVP goal is:

```text
Prevent accidental host filesystem damage.
```

The MVP does not claim to provide strong sandboxing against malicious agents.

Vegas Rooms should not mount:

```text
/
 /home
host Docker socket
full ~/.ssh
browser profiles
cloud credential directories
arbitrary host directories
```

## MVP non-goals

The following are not part of the MVP:

```text
Claude harness support
custom harness plugin system
Podman support
macOS support
WSL2 support
registry images
API key management
secrets vault integration
browser profile isolation
network hardening
seccomp/AppArmor tuning
read-only root filesystem
deploy-key automation
Git signing
non-interactive prompt execution
vr pi .
```

## Development framework

Vegas Rooms is built using:

```text
Walking Skeleton + Capability Milestones
```

Each milestone proves one concrete capability end to end.

Risky work moves through:

```text
Spike      — prove it manually
Stabilize  — turn it into project structure or code
Ship       — document and make it usable
```

The guiding rule:

```text
Every milestone must make Vegas Rooms more runnable.
```


