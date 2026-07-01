# Vegasroom Post-MVP Options

## Context

Vegasroom has reached MVP after M1–M5.

The current MVP proves:

```text
Pi runs inside an ephemeral rootless Docker container.
The Rust CLI wraps the runtime as vr.
vr init creates and repairs host state.
vr doctor reports readiness.
vr pi launches Pi.
vr shell opens the same room runtime for debugging.
SSH agent forwarding works when available.
Git-over-SSH works without copying private keys into the container.
Pi login works and persists across ephemeral container launches.
```

The current implementation intentionally preserves these MVP tradeoffs:

```text
container runs as root inside rootless Docker
network_mode=host is used
build.network=host is used
workspace is mounted read-write
Pi state/auth is mounted read-write
SSH agent forwarding is powerful
provider/API-key handling is out of scope
Claude support is deferred
hardening is deferred
```

The canonical project naming is:

```text
repository/package name: vegasroom
user-facing name: Vegasroom
CLI command: vr
state directory: ~/.vegasroom
default harness: Pi
MVP runtime: Linux + Docker + rootless Docker context
```

Post-MVP work should build on the proven runtime. Do not redesign the M1–M5 model unless a specific defect requires it.

---

# Option A — Harden the room runtime

## Goal

Improve the security posture of the container runtime without breaking the current working Pi, SSH, Git, and login flows.

## Scope

```text
move away from container-root runtime
reduce container capabilities
review host networking
explore Docker bridge networking again after rootless issues are understood
add safer mount policy
add read-only workspace option
add dangerous path warnings
document trust boundaries more explicitly
```

## Motivation

The MVP is functional but not hardened. It uses root inside the container, host networking, read-write mounts, and SSH agent forwarding. These choices were acceptable for proving the core workflow but should be revisited before broader use.

## Constraints

```text
do not break Pi login persistence
do not break SSH agent forwarding
do not break Git-over-SSH
do not reintroduce the M1 bind-mount failures
do not switch networking models without a proof
do not describe the result as a hardened sandbox until it is actually hardened
```

## Candidate work

```text
test non-root container runtime again with the final mount model
add UID/GID mapping options
make workspace read-only optionally
warn before mounting broad host paths
block obviously dangerous workspace paths
reduce Linux capabilities
review whether host networking is still required after M4
document residual risk
```

## Acceptance criteria

```text
Pi still launches.
Pi login still persists.
Git-over-SSH still works.
vr shell still works.
The container no longer needs root, or the reason root remains is documented.
Host networking is reduced or the reason it remains is documented.
Risky mount paths are warned or blocked.
Security docs are updated honestly.
```

---

# Option B — Add a second harness

## Goal

Prove Vegasroom can support more than Pi by adding a second real harness, likely Claude.

## Scope

```text
add vr claude
validate the harness abstraction with a real second harness
keep the config-driven model simple
avoid premature plugin architecture
reuse the proven Docker/Compose runtime model
preserve per-harness state isolation
```

## Motivation

The MVP currently functions as a Pi wrapper. Adding a second harness proves that Vegasroom is a general room system rather than a single-agent container launcher.

## Constraints

```text
do not build a harness marketplace
do not over-abstract before the second harness proves the abstraction
do not break vr pi
do not mix Pi and Claude state
do not add provider/API-key handling unless required by the harness milestone
```

## Candidate work

```text
add harness/claude/Dockerfile
add claude service or Compose override
add config.harness.claude
add vr claude
add Claude state directories
add doctor checks for the Claude image/state
document Claude limitations
```

## Acceptance criteria

```text
vr pi still works.
vr shell still works for Pi.
vr claude launches the second harness.
Pi and Claude state are isolated.
The config model remains understandable.
The runtime remains Docker/Compose-based.
```

---

# Option C — Improve ergonomics

## Goal

Make the source-based MVP easier and smoother to use.

## Scope

```text
add vr pi .
add vr pi <workspace>
add vr shell .
add vr shell <workspace>
add clearer working-directory behavior
add named or path-based workspace selection
add better install instructions
add a simple install script if useful
add release artifacts
improve doctor remediation
improve command output consistency
improve first-run guidance
make common errors easier to fix
```

## Workspace selection

### Current MVP behavior

```text
Vegasroom mounts the default workspace directory into the room:

~/.vegasroom/workspace -> /workspace
```

### Desired post-MVP behavior

```text
vr pi
  uses the default workspace

vr pi .
  mounts the current host directory as /workspace

vr pi my-git-repo
  mounts ~/.vegasroom/workspace/my-git-repo as /workspace

vr pi /absolute/path/to/repo
  mounts that absolute host path as /workspace

vr shell my-git-repo
  opens a debug shell with that same workspace mounted
```

### Suggested commands

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

### Path resolution rules

```text
no workspace argument:
  use configured default workspace:
  ~/.vegasroom/workspace

".":
  use the current host working directory

relative name without slash:
  resolve under configured workspace root:
  ~/.vegasroom/workspace/<name>

relative path with slash:
  resolve relative to current host working directory

absolute path:
  use the absolute path directly

~ path:
  expand against the host home directory
```

### Examples

```text
vr pi
  ~/.vegasroom/workspace

vr pi .
  current directory

vr pi my-git-repo
  ~/.vegasroom/workspace/my-git-repo

vr pi projects/my-git-repo
  ./projects/my-git-repo relative to current directory

vr pi ~/workspace/my-git-repo
  /home/<user>/workspace/my-git-repo

vr pi /home/dan/workspace/my-git-repo
  /home/dan/workspace/my-git-repo
```

### Safety behavior

```text
do not create arbitrary absolute paths silently
do not create parent directories outside ~/.vegasroom/workspace silently
do not allow mounting dangerous host paths without a warning
do not mount / as a workspace
do not mount the user home directory without a warning
do not mount ~/.ssh
do not mount ~/.config
do not mount ~/.aws, ~/.gcloud, ~/.kube, or similar credential directories
```

For this case:

```bash
vr pi my-git-repo
```

it is reasonable to create:

```text
~/.vegasroom/workspace/my-git-repo
```

if missing, because it lives inside the Vegasroom-managed workspace root.

For this case:

```bash
vr pi /some/external/path
```

prefer a clear failure if the path does not exist:

```text
FAIL: Workspace path does not exist: /some/external/path
Create it first or choose an existing directory.
```

### Compose implementation model

Keep the base Compose file stable by using an environment variable for the host workspace source:

```yaml
volumes:
  - type: bind
    source: ${VR_WORKSPACE:-${HOME}/.vegasroom/workspace}
    target: /workspace
```

Then the Rust wrapper invokes Compose with:

```text
VR_WORKSPACE=/resolved/host/path
```

This avoids rewriting `compose.yaml` per launch.

## Acceptance criteria

```text
vr pi still uses the default workspace.
vr shell still uses the default workspace.
vr pi . mounts the current directory as /workspace.
vr shell . mounts the current directory as /workspace.
vr pi my-git-repo mounts ~/.vegasroom/workspace/my-git-repo as /workspace.
vr shell my-git-repo mounts ~/.vegasroom/workspace/my-git-repo as /workspace.
absolute existing paths can be mounted explicitly.
missing external paths fail clearly.
dangerous paths produce warnings or failures.
the M1–M5 runtime model is preserved.
```

---

# Option D — Managed SSH agent

## Goal

Make Git-over-SSH work without requiring the user to understand `ssh-agent`, `SSH_AUTH_SOCK`, or `ssh-add`.

## Current MVP behavior

```text
Vegasroom forwards an existing host SSH_AUTH_SOCK when available.
The user is responsible for starting ssh-agent and loading keys.
Private keys are not copied into the container.
Host ~/.ssh is not mounted into the container.
```

## Desired post-MVP behavior

```text
vr ssh detects SSH keys on the host.
vr ssh lets the user select which keys Vegasroom should use.
Vegasroom stores the selected key paths or key metadata until changed.
vr pi and vr shell can start a temporary ssh-agent automatically.
Vegasroom adds the selected keys to that temporary agent.
Vegasroom forwards only the temporary agent socket into the room.
Vegasroom stops the temporary agent when the room exits.
```

## Possible commands

```bash
vr ssh status
vr ssh detect
vr ssh configure
vr ssh list
vr ssh add
vr ssh remove
vr ssh test
```

## Suggested workflow

```bash
vr ssh configure
vr doctor
vr shell
```

## Example behavior

```text
1. Vegasroom scans ~/.ssh for likely private keys.
2. Vegasroom shows key names, public fingerprints, and comments.
3. User selects one or more keys.
4. Vegasroom stores the selected key references in ~/.vegasroom/config.yaml.
5. On vr pi or vr shell, Vegasroom starts a scoped ssh-agent.
6. Vegasroom runs ssh-add for the selected keys.
7. The temporary SSH_AUTH_SOCK is forwarded into the container.
8. When the room exits, Vegasroom kills the temporary ssh-agent.
```

## Security rules

```text
do not copy private keys into the container
do not mount host ~/.ssh into the container
do not store key passphrases
do not store decrypted private keys
do not silently add every key
do not leave the managed ssh-agent running after the room exits unless explicitly requested
```

## Important limitation

```text
The private keys remain on the host, but Vegasroom must read selected host key files on the host in order to run ssh-add.
If a key is passphrase-protected, ssh-add should prompt through the host terminal or host askpass flow.
```

## Acceptance criteria

```text
vr ssh detect lists candidate keys without exposing private key material.
vr ssh configure lets the user select keys.
Selected key references persist until changed.
vr shell can start a temporary ssh-agent.
ssh-add -l works inside the room.
ssh -T git@github.com works inside the room.
git clone over SSH works inside /workspace.
Private keys are not mounted into the container.
Host ~/.ssh is not mounted into the container.
Temporary ssh-agent is removed after room exit unless explicitly configured otherwise.
```

This is one of the strongest immediate post-MVP tracks because it removes a major setup burden while preserving the MVP promise that private keys are not copied into the room.

---

# Option E — Git identity and repository workflows

## Goal

Improve Git-specific guidance after the SSH agent workflow is stable.

## Scope

```text
document deploy-key workflow
add per-repo SSH guidance
add GitHub known_hosts guidance
add git clone troubleshooting
explore Git signing later
support safer test-push workflows
```

## Motivation

M3 proved Git-over-SSH works. Option D can make SSH-agent setup easy. This track makes repository identity and Git workflows clearer.

## Candidate work

```text
document personal-key vs deploy-key tradeoffs
document GitHub host key prompts
document cloning private repos into workspace
document per-repo git config
document safe push tests
add doctor hints for common Git SSH failures
add optional vr git test command later
```

## Acceptance criteria

```text
Users can understand how to use personal SSH keys safely.
Users can understand deploy-key workflows.
GitHub host key behavior is documented.
Git clone and fetch troubleshooting is documented.
Git signing is explicitly deferred or scoped separately.
```

---

# Option F — Host bootstrap

## Goal

Make Vegasroom easier to set up on a fresh Linux machine by detecting, installing, and configuring host dependencies where practical.

## Current MVP behavior

```text
The user must already have Docker, Docker Compose, and a usable rootless Docker context.
Vegasroom diagnoses missing pieces with vr doctor.
The user fixes Docker/rootless setup manually.
```

## Desired post-MVP behavior

```text
vr doctor detects missing host dependencies.
vr bootstrap guides or performs safe setup steps.
vr bootstrap can install or configure rootless Docker where supported.
vr init remains a safe state-directory repair command.
```

## Recommended commands

```bash
vr bootstrap
vr bootstrap --check
vr bootstrap --docker
vr bootstrap --rootless-docker
vr bootstrap --print-only
```

## Suggested workflow

```text
1. Detect Linux distribution.
2. Detect Docker installation.
3. Detect Docker Compose support.
4. Detect rootless Docker support.
5. Detect whether the current user can run the rootless Docker daemon.
6. Detect whether a Docker context named rootless exists.
7. Offer to install or configure missing pieces.
8. Create or repair the rootless Docker context.
9. Run a trivial container proof.
10. Run vr doctor.
```

## Expected user flow

```bash
vr bootstrap
vr init --build
vr doctor
vr
```

For diagnostics only:

```bash
vr bootstrap --check
```

## Docker installation policy

`vr bootstrap` may support Docker installation, but should be conservative.

Rules:

```text
do not install Docker silently
do not run sudo without explicit confirmation
do not overwrite existing Docker configuration
do not replace a working Docker setup
do not assume one Linux distribution
prefer showing the exact commands before running them
provide --print-only for copy/paste installation
```

Useful supported paths:

```text
Fedora / RHEL-family
Debian / Ubuntu-family
Arch-family
manual fallback
```

The first version does not need to support every distribution. It can detect unsupported systems and print manual instructions.

## Rootless Docker setup

`vr bootstrap --rootless-docker` should focus on making this command work:

```bash
docker --context rootless info
```

Useful setup checks:

```text
dockerd-rootless-setuptool.sh exists
rootless Docker daemon is installed
user systemd is available
linger is enabled if needed
DOCKER_HOST is not conflicting
Docker context rootless exists
Docker context rootless points to the user daemon socket
rootless daemon can run hello-world
host networking works for the Vegasroom runtime
```

Possible setup actions:

```text
run dockerd-rootless-setuptool.sh install
enable/start the user Docker service
create a Docker context named rootless
verify docker --context rootless info
verify docker --context rootless run --rm --network host hello-world
```

## Alias and context handling

Avoid relying on shell aliases.

Vegasroom should keep using config:

```yaml
docker:
  context: rootless
```

and internally invoke:

```bash
docker --context rootless ...
```

If bootstrap creates a context, it should create the context name that Vegasroom expects:

```text
rootless
```

If a user already has a differently named rootless context, bootstrap should either update:

```text
~/.vegasroom/config.yaml
```

to use that context, or ask whether to create an additional context named:

```text
rootless
```

Do not depend on aliases like:

```bash
alias docker='docker --context rootless'
```

Aliases are shell-specific and do not reliably apply to subprocesses launched by `vr`.

## Relationship to `vr init`

Keep the split clear:

```text
vr init
  creates or repairs Vegasroom state only

vr bootstrap
  checks or modifies host dependencies

vr doctor
  reports readiness and exact remediation steps
```

`vr init --build` can remain available because building the local Pi image is part of project setup, not system installation.

Avoid making plain `vr init` install Docker. It should stay safe and repeatable.

## Acceptance criteria

```text
vr bootstrap --check reports Docker/rootless readiness clearly.
vr bootstrap --print-only prints exact install/setup commands for the detected system.
vr bootstrap --rootless-docker can configure the rootless Docker context on at least one supported Linux family.
vr doctor recognizes the resulting setup.
vr init --build succeeds after bootstrap.
vr launches Pi without the user manually constructing Docker commands.
```

## Security and UX notes

Document clearly:

```text
Installing Docker modifies the host.
Rootless Docker still runs a user daemon.
Rootless Docker is not equivalent to a hardened sandbox.
Vegasroom uses Docker through an explicit configured context, not shell aliases.
Users can review commands before running them.
```

---

# Suggested prioritization

## Highest immediate value

```text
1. Option C — Improve ergonomics: workspace selection
2. Option D — Managed SSH agent
3. Option F — Host bootstrap
```

These directly reduce the biggest remaining user friction:

```text
How do I open the right project?
How do I make SSH work?
How do I set up rootless Docker?
```

## Product expansion

```text
4. Option B — Add a second harness
```

This proves Vegasroom as a general room system.

## Security maturity

```text
5. Option A — Harden the room runtime
```

This should happen before broader distribution, but after the current workflow is stable enough to preserve.

## Git workflow maturity

```text
6. Option E — Git identity and repository workflows
```

This pairs well with managed SSH agent work.

---

# Recommended next milestone

The best immediate next milestone is:

```text
M6 — Workspace
```

Goal:

```text
Make vr pi <workspace> and vr shell <workspace> work safely and predictably.
```

Suggested follow-on milestones:

```text
M7 — Managed SSH
M8 — Bootstrap
M9 — Harden
M10 — Extend
```
