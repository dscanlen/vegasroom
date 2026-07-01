# Vegasroom

Vegasroom is an experimental CLI for launching AI agent harnesses inside ephemeral Docker containers.

M2 provides a minimal Rust command named `vr` that wraps the proven M1 Pi runtime.

## Current commands

```bash
cargo run -- init
cargo run -- doctor
cargo run -- pi
cargo run -- shell
```

When installed as `vr`:

```bash
vr init
vr doctor
vr pi
vr shell
vr
```

`vr` defaults to `vr pi`.

## Runtime assumptions

M2 preserves the M1 runtime decisions:

- Linux
- Docker with a configured `rootless` context
- Docker Compose
- host-network fallback for rootless Docker
- container-root runtime inside rootless Docker
- explicit bind mounts only
- Pi state under `~/.vegasroom/harness/pi`
- workspace under `~/.vegasroom/workspace`
- SSH directory mount at `~/.vegasroom/ssh`

## Build image

After `vr init`, build the local Pi image:

```bash
cargo run -- init --build
```

or directly:

```bash
docker --context rootless compose build pi
```

## State directory

Vegasroom creates or repairs:

```text
~/.vegasroom/
  config.yaml
  workspace/
  harness/pi/config/
  harness/pi/extensions/
  harness/pi/skills/
  harness/pi/sessions/
  ssh/
  cache/
```

Provider/API-key handling is intentionally out of scope for M2.

## SSH agent forwarding

M3 forwards the host ssh-agent socket when `SSH_AUTH_SOCK` points to a real socket.

The container sees:

```text
SSH_AUTH_SOCK=/tmp/vegasroom/ssh-agent.sock
```

Vegasroom does not copy SSH private keys into the container and does not mount the host `~/.ssh`. It mounts only the Vegas-managed SSH directory:

```text
~/.vegasroom/ssh -> /home/agent/.ssh
~/.vegasroom/ssh -> /root/.ssh
```

Forwarding the ssh-agent socket allows processes inside the container to request SSH signatures from identities loaded in the host agent while the socket is mounted.

### M3 proof commands

On the host:

```bash
echo "$SSH_AUTH_SOCK"
ssh-add -l
cargo run -- init
cargo run -- doctor
cargo run -- shell
```

Inside the room:

```bash
echo "$SSH_AUTH_SOCK"
ls -la /tmp/vegasroom
ssh-add -l
ssh -T git@github.com
cd /workspace
git clone git@github.com:OWNER/REPO.git
cd REPO
git fetch
```
