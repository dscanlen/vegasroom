# Vegasroom

Vegasroom is an experimental CLI for launching AI agent harnesses inside ephemeral Docker containers.

M4 provides a minimal Rust command named `vr` that wraps the proven M1 Pi runtime, forwards the host ssh-agent from M3, and supports Pi login persistence through the explicit Pi state mount.

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

M4 preserves the M1-M3 runtime decisions:

- Linux
- Docker with a configured `rootless` context
- Docker Compose
- host-network fallback for rootless Docker
- container-root runtime inside rootless Docker
- explicit bind mounts only
- Pi state under `~/.vegasroom/harness/pi`
- workspace under `~/.vegasroom/workspace`
- SSH directory mount at `~/.vegasroom/ssh`
- Pi auth state under `~/.vegasroom/harness/pi/config/auth.json` after `/login`

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

Provider/API-key handling is intentionally out of scope for M4. Pi native `/login` is used for subscription/provider auth.

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


## Pi login proof

M4 keeps browser work on the host where possible. The Compose service sets:

```text
BROWSER=echo
```

This encourages browser-login helpers to print the URL instead of trying to launch a browser inside the container. Open the printed URL on the host, complete login, return to Pi, and then exit/relaunch the room.

Pi auth state is expected to persist at:

```text
~/.vegasroom/harness/pi/config/auth.json
```

because the container path `/home/agent/.pi/agent` is mounted from `~/.vegasroom/harness/pi/config`.

### M4 proof commands

```bash
cargo run -- init
cargo run -- doctor
cargo run -- pi
```

Inside Pi:

```text
/login
```

After login, exit and relaunch:

```bash
cargo run -- pi
```

Then verify that Pi remains authenticated. For filesystem inspection:

```bash
cargo run -- shell
ls -la /home/agent/.pi/agent
find /home/agent/.pi/agent -maxdepth 2 -type f | sort
```

M4 does not mount host browser profiles and does not add provider API keys to Vegasroom config.
