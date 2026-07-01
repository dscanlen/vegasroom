# M1 Room Notes

## Milestone

M1 — Room

## Goal

Prove that Pi Agent Harness can run interactively inside an ephemeral rootless Docker container on Linux while preserving only explicit mounted state.

This milestone intentionally does not build the Rust `vr` CLI.

## Final status

M1 proof gate is complete.

The target command works on the Linux rootless Docker host:

```bash
docker --context rootless compose run --rm pi
```

Final observed Pi launch:

```text
pi v0.80.3
escape interrupt · ctrl+c/ctrl+d clear/exit · / commands · ! bash · ctrl+o more
Press ctrl+o to show full startup help and loaded resources.

Pi can explain its own features and look up its docs. Ask it how to use or extend Pi.

Warning: No models available. Use /login to log into a provider via OAuth or API key.
/workspace
0.0%/0 (auto)
```

The "No models available" warning is acceptable for M1. Provider login/API key handling is out of scope for this milestone.

## Final implementation

Project files:

```text
vegasroom/
  compose.yaml
  harness/
    pi/
      Dockerfile
  docs/
    m1-room-notes.md
```

Final runtime properties:

- Runs under rootless Docker context.
- Uses Compose as the inspectable runtime definition.
- Runs Pi interactively with TTY/stdin enabled.
- Mounts workspace at `/workspace`.
- Persists Pi state only through explicit `~/.vegas-rooms` mounts.
- Uses an ephemeral `docker compose run --rm` container.
- Runs as container root for M1 to avoid rootless bind-mount UID/GID friction.
- Installs `fd` and `ripgrep` in the image so Pi does not try to download helper binaries into mounted state at startup.

## Final Docker decisions

### Base image

```dockerfile
FROM node:24-bookworm-slim
```

Reason: Pi's current containerization guidance uses the Node 24 Bookworm slim image, and Pi is distributed as an npm package.

### Pi installation

```dockerfile
RUN npm install -g --ignore-scripts @earendil-works/pi-coding-agent
```

Observed version:

```text
0.80.3
```

### Runtime packages

Final package set:

```text
ca-certificates
fd-find
git
openssh-client
ripgrep
```

Reasoning:

- `ca-certificates`: outbound HTTPS and npm/Pi network access.
- `fd-find`: Pi looks for `fd` at startup.
- `ripgrep`: Pi looks for `rg` at startup.
- `git`: expected source/workspace tool.
- `openssh-client`: SSH/Git-over-SSH readiness.

Debian exposes `fd` as `/usr/bin/fdfind`, so the Dockerfile creates:

```bash
ln -sf /usr/bin/fdfind /usr/local/bin/fd
```

### Removed from M1 image

These were removed or intentionally not included:

```text
bash
curl
dnsutils
iproute2
iputils-ping
less
nano
procps
tini
```

Notes:

- `tini` is not needed for the M1 proof. Pi runs directly as PID 1.
- Network proof uses Node's built-in `fetch`, so `curl` is not required.
- Shell checks use `/bin/sh`, so `bash` is not required.

## Final Compose decisions

### Rootless networking fallback

Compose includes host-network fallbacks:

```yaml
build:
  network: ${VR_PI_BUILD_NETWORK:-host}
network_mode: ${VR_PI_NETWORK_MODE:-host}
```

Reason: the first target-host build failed before the Dockerfile command executed because rootless Docker could not create a default bridge/veth endpoint:

```text
failed to create endpoint ... on network bridge
failed to add the host (...) <=> sandbox (...) pair interfaces: operation not supported
```

Using host network mode allowed the build/run path to progress on the target rootless Docker host.

### Runtime user

Compose includes:

```yaml
user: "0:0"
```

Reason: using the built-in non-root `node` user caused write friction with explicit bind-mounted state under rootless Docker. M1 uses container root under rootless Docker as a pragmatic proof choice. Rootless Docker maps container root through the rootless user namespace, so this is not host root.

M2 may revisit UID/GID handling for a cleaner non-root container user strategy.

## Final state model

Pi global state root inside the container:

```text
/home/agent/.pi/agent
```

Pi session directory inside the container:

```text
/home/agent/.pi/sessions
```

M1 sets:

```text
PI_CODING_AGENT_SESSION_DIR=/home/agent/.pi/sessions
```

Host state layout:

```text
~/.vegas-rooms/
  harness/
    pi/
      config/       # mounted at /home/agent/.pi/agent
      extensions/   # mounted at /home/agent/.pi/extensions
      skills/       # mounted at /home/agent/.pi/skills
      sessions/     # mounted at /home/agent/.pi/sessions
  workspace/        # mounted at /workspace
  ssh/              # mounted at /home/agent/.ssh
    known_hosts
  cache/            # mounted at /home/agent/.cache
```

Required persistent paths:

| Host path | Container path | Reason |
|---|---|---|
| `~/.vegas-rooms/workspace` | `/workspace` | Host-visible workspace files. |
| `~/.vegas-rooms/harness/pi/config` | `/home/agent/.pi/agent` | Pi settings, auth, trust, and package/helper state. |
| `~/.vegas-rooms/harness/pi/sessions` | `/home/agent/.pi/sessions` | Pi sessions via `PI_CODING_AGENT_SESSION_DIR`. |
| `~/.vegas-rooms/harness/pi/extensions` | `/home/agent/.pi/extensions` | Vegas-managed extensions. |
| `~/.vegas-rooms/harness/pi/skills` | `/home/agent/.pi/skills` | Vegas-managed skills. |
| `~/.vegas-rooms/ssh` | `/home/agent/.ssh` | SSH known hosts and later SSH-related state. |
| `~/.vegas-rooms/cache` | `/home/agent/.cache` | Runtime/tool cache. |

Important mount correction:

- Do not bind-mount `~/.vegas-rooms/ssh/known_hosts` directly to `/home/agent/.ssh/known_hosts`.
- Bind-mount the SSH directory instead.
- Docker may create missing bind sources as directories, which causes file-vs-directory mount errors for missing `known_hosts` files.

## Host preflight

Run once on the Linux host:

```bash
mkdir -p \
  ~/.vegas-rooms/harness/pi/config \
  ~/.vegas-rooms/harness/pi/extensions \
  ~/.vegas-rooms/harness/pi/skills \
  ~/.vegas-rooms/harness/pi/sessions \
  ~/.vegas-rooms/workspace \
  ~/.vegas-rooms/ssh \
  ~/.vegas-rooms/cache

touch ~/.vegas-rooms/ssh/known_hosts
chmod 700 ~/.vegas-rooms/ssh
chmod 644 ~/.vegas-rooms/ssh/known_hosts
```

If a previous run accidentally created `known_hosts` as a directory:

```bash
rm -rf ~/.vegas-rooms/ssh/known_hosts
mkdir -p ~/.vegas-rooms/ssh
touch ~/.vegas-rooms/ssh/known_hosts
chmod 700 ~/.vegas-rooms/ssh
chmod 644 ~/.vegas-rooms/ssh/known_hosts
```

## Attempt log

### Attempt 1 — local Pi package inspection outside Docker

Command:

```bash
npm install -g --ignore-scripts @earendil-works/pi-coding-agent
pi --version
```

Result:

```text
0.80.3
```

Findings:

- npm installation works with `--ignore-scripts`.
- `pi --version` works.
- Pi creates state under `~/.pi/agent`.

Decision:

- Install Pi globally in the Docker image with npm.
- Persist `/home/agent/.pi/agent` in the container.

### Attempt 2 — local interactive TTY smoke outside Docker

Result:

- Pi entered its interactive TUI.
- Pi warned cleanly when no provider/model credentials were configured.
- Pi required a writable home/state root.

Decision:

- M1 can prove interactive launch without solving API key management.

### Attempt 3 — Docker unavailable in assistant execution environment

Result:

```text
bash: docker: command not found
```

Decision:

- Final proof gate must run on the target Linux rootless Docker host.

### Attempt 4 — target rootless build failed on default bridge/veth

Command:

```bash
docker --context rootless compose build pi
```

Result:

```text
failed to create endpoint ... on network bridge
failed to add the host (...) <=> sandbox (...) pair interfaces: operation not supported
```

Decision:

- Add `build.network: host` and `network_mode: host` fallbacks in Compose.

### Attempt 5 — UID/GID conflict in Node base image

Result:

```text
groupadd: GID '1000' already exists
```

Reason:

- The Node base image already reserves UID/GID 1000 for the built-in `node` user/group.

Decision:

- Stop creating a new `agent` user.
- Use stable `HOME=/home/agent`.
- M1 later switched runtime to container root to handle bind-mount write behavior under rootless Docker.

### Attempt 6 — SSH known_hosts mount mismatch

Result:

```text
error mounting "/home/dan/.vegas-rooms/ssh/known_hosts" ... not a directory
Are you trying to mount a directory onto a file (or vice-versa)?
```

Reason:

- Docker created a missing bind source as a directory.

Decision:

- Mount `~/.vegas-rooms/ssh` to `/home/agent/.ssh` instead of mounting the `known_hosts` file directly.

### Attempt 7 — Pi launched but helper downloads failed

Result:

```text
fd not found. Downloading...
ripgrep not found. Downloading...
Failed to download fd: EACCES: permission denied, mkdir '/home/agent/.pi/agent/bin'
Failed to download ripgrep: EACCES: permission denied, mkdir '/home/agent/.pi/agent/bin'
```

Decision:

- Install `fd-find` and `ripgrep` in the image.
- Symlink `/usr/local/bin/fd` to `/usr/bin/fdfind`.
- Run as container root for M1 to avoid rootless bind-mount permission friction.

### Attempt 8 — final M1 proof gate

#### 1. Image tools

Command:

```bash
docker --context rootless compose run --rm pi sh -lc '
  id
  command -v pi
  command -v fd
  command -v rg
  pi --version
  fd --version
  rg --version | head -1
'
```

Result:

```text
uid=0(root) gid=0(root) groups=0(root)
/usr/local/bin/pi
/usr/local/bin/fd
/usr/bin/rg
0.80.3
fdfind 8.6.0
ripgrep 13.0.0
```

Status: passed.

#### 2. Workspace persistence

Command:

```bash
docker --context rootless compose run --rm pi sh -lc '
  date -Iseconds > /workspace/m1-workspace-proof.txt
'
cat ~/.vegas-rooms/workspace/m1-workspace-proof.txt
```

Result:

```text
2026-07-01T04:08:02+00:00
```

Status: passed.

#### 3. Pi state persistence

Command:

```bash
docker --context rootless compose run --rm pi sh -lc '
  mkdir -p /home/agent/.pi/agent/bin
  date -Iseconds > /home/agent/.pi/agent/m1-state-proof.txt
'
cat ~/.vegas-rooms/harness/pi/config/m1-state-proof.txt
```

Result:

```text
2026-07-01T04:08:36+00:00
```

Status: passed.

#### 4. Ephemeral container removal

Command:

```bash
docker --context rootless ps -a --filter name=vegasroom-pi-run
docker --context rootless compose run --rm pi sh -lc 'echo m1 container removal proof'
docker --context rootless ps -a --filter name=vegasroom-pi-run
```

Result:

```text
CONTAINER ID   IMAGE     COMMAND   CREATED   STATUS    PORTS     NAMES
Container vegasroom-pi-run-f80903fc6cba Creating
Container vegasroom-pi-run-f80903fc6cba Created
m1 container removal proof
CONTAINER ID   IMAGE     COMMAND   CREATED   STATUS    PORTS     NAMES
```

Status: passed.

#### 5. Outbound network

Command:

```bash
docker --context rootless compose run --rm pi sh -lc '
  node -e "fetch(\"https://pi.dev\").then(r => console.log(r.status)).catch(e => { console.error(e); process.exit(1) })"
'
```

Result:

```text
200
```

Status: passed.

#### 6. Interactive Pi launch

Command:

```bash
docker --context rootless compose run --rm pi
```

Result:

```text
pi v0.80.3
escape interrupt · ctrl+c/ctrl+d clear/exit · / commands · ! bash · ctrl+o more
Press ctrl+o to show full startup help and loaded resources.

Pi can explain its own features and look up its docs. Ask it how to use or extend Pi.

Warning: No models available. Use /login to log into a provider via OAuth or API key.
/workspace
0.0%/0 (auto)
```

Status: passed.

## M1 answers

### Pi installation

- Pi is installed with npm in the image.
- Install command: `npm install -g --ignore-scripts @earendil-works/pi-coding-agent`.
- Observed Pi version: `0.80.3`.
- Base image: `node:24-bookworm-slim`.
- Required runtime helpers in final M1 image: `fd`, `rg`, Git, SSH client, CA certs.
- Pi can launch in the container after those helpers are present.

### Pi filesystem behavior

- Pi global state root: `/home/agent/.pi/agent`.
- Pi stores/authenticates provider state under the global state root; provider login was not performed in M1.
- Pi sessions are redirected to `/home/agent/.pi/sessions` with `PI_CODING_AGENT_SESSION_DIR`.
- Vegas mounts explicit paths for config, extensions, skills, sessions, workspace, SSH, and cache.
- `/workspace` persists to `~/.vegas-rooms/workspace`.
- `/home/agent/.pi/agent` persists to `~/.vegas-rooms/harness/pi/config`.

### Runtime behavior

- Pi starts correctly in an interactive TTY.
- Pi launches from `/workspace`.
- Pi needs writable HOME/state.
- Pi expects `fd` and `rg`; installing them avoids startup downloads.
- No provider/model configured yet; the no-model warning is expected and out of scope.

### Docker behavior

- Rootless Docker can build and run the image with the host-network fallback.
- Bind mounts work with the final container-root M1 strategy.
- `docker compose run --rm` removes the run container after exit.
- Compose is good enough to become the basis for M2.

### Network behavior

- Outbound HTTPS works from the container.
- `fetch("https://pi.dev")` returned HTTP `200`.
- DNS therefore works for the tested path.
- `host.docker.internal` remains a nice-to-have check and is not required to close M1.

## M1 proof checklist

| Checkpoint | Status |
|---|---:|
| Pi installation inside the image | Passed |
| Interactive TTY launch | Passed |
| Ephemeral container removal after exit | Passed |
| Workspace mount persistence | Passed |
| Pi config/state persistence | Passed |
| Pi session persistence path configured | Passed |
| Basic outbound network access | Passed |
| Basic host access via `host.docker.internal` | Not required / not closed |
| Git installed | Passed |
| SSH client installed | Passed |
| `fd` installed | Passed |
| `rg` installed | Passed |

## Stop condition

M1 is complete.

Do not continue into Rust CLI work as part of M1.

M2 may now wrap this proven Docker/Compose flow in Rust.

## M2 carry-forward notes

Important implementation constraints for M2:

- Preserve the exact mount model proven here before adding abstractions.
- Create/repair `~/.vegas-rooms` directories before invoking Compose/Docker.
- Treat `~/.vegas-rooms/ssh` as a directory mount, not a single-file mount.
- Use rootless Docker context explicitly: `docker --context rootless ...`.
- Keep `build.network=host` and `network_mode=host` defaults until rootless bridge behavior is separately solved.
- Keep M1's container-root runtime unless M2 deliberately introduces and tests a UID/GID mapping strategy.
- Do not add provider/API-key management in M2 unless it is explicitly scoped.
