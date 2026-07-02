# Vegasroom TODO

## Current baseline

Vegasroom has reached MVP. The current model is:

```text
repository/package name: vegasroom
user-facing name: Vegasroom
CLI command: vr
state directory: ~/.vegasroom
default harness: Pi
MVP runtime: Linux + Docker + rootless Docker context
```

The MVP currently proves:

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

Known MVP tradeoffs to preserve or address deliberately:

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

Post-MVP work should build on the proven runtime. Do not redesign the M1–M5 model unless a specific defect requires it.

---

## Priority order

1. **M6 — Workspace and CLI ergonomics** **DONE**
2. **M7 — Managed SSH and repo-specific Git access** **OUT OF SCOPE FOR NOW**
3. **M8 — Host bootstrap** **OUT OF SCOPE FOR NOW**
4. **M9 — Runtime hardening**
5. **M10 — Second harness**
6. **Git workflow polish**

---

## M7 — Managed SSH and repo-specific Git access

### Goal

Support multiple SSH keys while ensuring repo-specific deploy keys are used for the right repository.

### Problem

```text
SSH agents can hold multiple keys.
GitHub deploy keys are often repository-specific.
If the wrong key is offered first, auth can fail or select the wrong identity.
Vegasroom currently forwards an agent socket but does not manage per-repo SSH identity selection.
```

### Constraints

```text
do not copy private keys into the container.
do not mount host ~/.ssh into the container.
do not store private key material or passphrases.
preserve managed temporary ssh-agent lifecycle.
private keys remain on the host and are only loaded into the temporary managed agent.
the room receives only the agent socket and generated non-secret SSH config/state.
```

### Approaches to investigate

#### 1. Room-local SSH config with host aliases

Generate room-local SSH config under:

```text
~/.vegasroom/ssh/config
```

Example:

```text
Host github.com-owner-repo
  HostName github.com
  User git
  IdentitiesOnly yes
  IdentityAgent /tmp/vegasroom/ssh-agent.sock
```

Then document clone URLs using the alias:

```bash
git clone git@github.com-owner-repo:OWNER/REPO.git
```

#### 2. Per-repo Git `core.sshCommand`

Use per-repo Git config inside `/workspace`:

```bash
git config core.sshCommand 'ssh -o IdentitiesOnly=yes -o IdentityAgent=/tmp/vegasroom/ssh-agent.sock ...'
```

#### 3. Repo-key helper commands

Explore helper commands such as:

```bash
vr ssh repo add OWNER/REPO ~/.ssh/deploy_key_for_repo
vr ssh repo list
vr ssh repo remove OWNER/REPO
```

#### 4. Selected-key metadata

Explore whether selected-key metadata can include intended repo or host patterns.

### M7 acceptance criteria

```text
a user can configure multiple deploy keys.
Git operations for repo A use repo A's deploy key.
Git operations for repo B use repo B's deploy key.
private keys remain on the host.
private keys are only loaded into the temporary managed agent.
the container receives only the agent socket and generated non-secret SSH config/state.
Git-over-SSH still works for the simple single-key case.
```

---

## M8 — Host bootstrap

### Goal

Make Vegasroom easier to set up on a fresh Linux machine by detecting, installing, and configuring host dependencies where practical.

### Current MVP behavior

```text
The user must already have Docker, Docker Compose, and a usable rootless Docker context.
Vegasroom diagnoses missing pieces with vr doctor.
The user fixes Docker/rootless setup manually.
```

### Desired behavior

```text
vr doctor detects missing host dependencies.
vr bootstrap guides or performs safe setup steps.
vr bootstrap can install or configure rootless Docker where supported.
vr init remains a safe state-directory repair command.
```

### Commands to support

```bash
vr bootstrap
vr bootstrap --check
vr bootstrap --docker
vr bootstrap --rootless-docker
vr bootstrap --print-only
```

### Suggested workflow

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

Expected user flow:

```bash
vr bootstrap
vr init --build
vr doctor
vr
```

Diagnostics only:

```bash
vr bootstrap --check
```

### Docker installation policy

```text
do not install Docker silently.
do not run sudo without explicit confirmation.
do not overwrite existing Docker configuration.
do not replace a working Docker setup.
do not assume one Linux distribution.
prefer showing the exact commands before running them.
provide --print-only for copy/paste installation.
```

Useful supported paths:

```text
Fedora / RHEL-family
Debian / Ubuntu-family
Arch-family
manual fallback
```

The first version does not need to support every distribution. It can detect unsupported systems and print manual instructions.

### Rootless Docker setup

`vr bootstrap --rootless-docker` should focus on making this work:

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

### Alias and context handling

Avoid relying on shell aliases. Vegasroom should keep using config:

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

### Relationship to `vr init`

```text
vr init
  creates or repairs Vegasroom state only

vr bootstrap
  checks or modifies host dependencies

vr doctor
  reports readiness and exact remediation steps
```

`vr init --build` can remain available because building the local Pi image is part of project setup, not system installation.

Plain `vr init` should not install Docker. It should stay safe and repeatable.

### M8 acceptance criteria

```text
vr bootstrap --check reports Docker/rootless readiness clearly.
vr bootstrap --print-only prints exact install/setup commands for the detected system.
vr bootstrap --rootless-docker can configure the rootless Docker context on at least one supported Linux family.
vr doctor recognizes the resulting setup.
vr init --build succeeds after bootstrap.
vr launches Pi without the user manually constructing Docker commands.
Installing Docker is documented as a host modification.
Rootless Docker is documented as a user daemon, not a hardened sandbox.
Users can review commands before running them.
```

---

## M9 — Runtime hardening

### Goal

Improve the security posture of the container runtime without breaking the current working Pi, SSH, Git, and login flows.

### Scope

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

### Constraints

```text
do not break Pi login persistence.
do not break SSH agent forwarding.
do not break Git-over-SSH.
do not reintroduce the M1 bind-mount failures.
do not switch networking models without a proof.
do not describe the result as a hardened sandbox until it is actually hardened.
```

### Candidate work

```text
test non-root container runtime again with the final mount model.
add UID/GID mapping options.
make workspace read-only optionally.
warn before mounting broad host paths.
block obviously dangerous workspace paths.
reduce Linux capabilities.
review whether host networking is still required after M4.
document residual risk.
```

### M9 acceptance criteria

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

## M10 — Second harness

### Goal

Prove Vegasroom can support more than Pi by adding a second real harness, likely Claude.

### Scope

```text
add vr claude
validate the harness abstraction with a real second harness
keep the config-driven model simple
avoid premature plugin architecture
reuse the proven Docker/Compose runtime model
preserve per-harness state isolation
```

### Constraints

```text
do not build a harness marketplace.
do not over-abstract before the second harness proves the abstraction.
do not break vr pi.
do not mix Pi and Claude state.
do not add provider/API-key handling unless required by the harness milestone.
```

### Candidate work

```text
add harness/claude/Dockerfile
add claude service or Compose override
add config.harness.claude
add vr claude
add Claude state directories
add doctor checks for the Claude image/state
document Claude limitations
```

### M10 acceptance criteria

```text
vr pi still works.
vr shell still works for Pi.
vr claude launches the second harness.
Pi and Claude state are isolated.
The config model remains understandable.
The runtime remains Docker/Compose-based.
```

---

## Git workflow polish

### Goal

Commits created from this environment should use the intended Git/GitHub profile identity instead of `root <root@...>`.

### Current symptom

```text
Committer: root <root@nomad.localdomain>
```

### Tasks

```text
Configure repo-local or environment-level Git identity.
Prefer repo-local config if this should only affect Vegasroom.
Use the GitHub profile name/email intended for this project.
Consider GitHub noreply email if privacy is desired.
Amend any local-only commits before pushing when identity is wrong.
Document how automation should identify itself in commits.
```

Useful commands:

```bash
git config user.name "<GitHub display name>"
git config user.email "<GitHub email or noreply email>"
git commit --amend --reset-author
```

### Open question

```text
Should future agent-created commits always use the user's Git identity, or a distinct bot/co-author identity?
```

### Acceptance criteria

```text
New commits no longer default to root <root@...>.
The intended repo-local Git identity is documented.
Local-only commits with the wrong identity can be amended safely.
Agent-created commit identity policy is explicit.
```

---

## Documentation updates

Keep documentation aligned as these tasks land:

```text
Document final vr pi and vr shell workspace syntax.
Document Pi option pass-through syntax.
Document SSH deploy-key setup and repo alias behavior.
Document bootstrap safety policy and supported platforms.
Document current trust boundaries and residual runtime risk.
Document Git identity setup for automation-created commits.
Keep POST-MVP-OPTIONS.md as background planning, or replace it with this TODO once the repo is ready.
```
