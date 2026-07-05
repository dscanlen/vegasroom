# Vegasroom TODO

This file is the canonical planning document for Vegasroom. `POST-MVP-OPTIONS.md` has been retired; useful post-MVP planning has been folded into this TODO.

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
Managed temporary ssh-agent support works for selected keys.
Git-over-SSH works without copying private keys into the container.
Pi login works and persists across ephemeral container launches.
Workspace selection works for default, named, relative, tilde, current-directory, and absolute paths.
```

Known MVP tradeoffs to preserve or address deliberately:

```text
container runs as root inside rootless Docker
network_mode=host is used by default
build.network=host is used by default
workspace is mounted read-write
Pi state/auth is mounted read-write
SSH agent forwarding is powerful
provider/API-key handling is out of scope
Claude support is deferred
hardening is deferred
```

Post-MVP work should build on the proven runtime. Do not redesign the M1-M6 model unless a specific defect requires it.

---

## Active recommended work order

### 1. Make `vr doctor` faster and less repetitive

**Status:** DONE

`vr doctor` now batches room-side checks into one base container probe plus one SSH probe when SSH is planned. The probes emit structured output that is parsed by the host while preserving readable PASS/WARN/FAIL doctor output.

Completed:

```text
batched Pi config, Pi sessions, internet, and room Git identity checks into one Compose run
batched SSH_AUTH_SOCK, ssh-add availability, and ssh-add -l checks into one SSH-aware Compose run
avoided repeated managed SSH setup during doctor container checks
kept host checks and container checks separate
preserved actionable PASS/WARN/FAIL remediation text
```

Acceptance criteria:

```text
vr doctor remains clear and accurate
container checks complete materially faster
managed SSH passphrase prompts are not repeated unnecessarily
failures still include actionable remediation text
```

### 2. Simplify and make config fields honest

**Status:** DONE

Config defaults now only include fields with current runtime effect. Legacy/future-facing fields are ignored if present and no longer appear in generated defaults or docs examples.

Completed:

```text
removed default_harness, paths.root, harness.pi.enabled, harness.pi.ssh_agent, and commented Claude config from generated defaults
made harness.pi.image control the Compose image through VR_PI_IMAGE
made harness.pi.command control vr pi with and without Pi arguments
made harness.pi.network control runtime/build networking through VR_PI_NETWORK_MODE and VR_PI_BUILD_NETWORK
updated README and docs/config.md to match implementation
added config/runtime tests for legacy ignored fields, Pi command args, and Compose env wiring
```

Acceptance criteria:

```text
every documented active config field has runtime effect
every future-facing field is clearly labeled or removed from defaults
README and docs/config.md match implementation
```

### 3. Refactor large modules after tests are in place

**Status:** IN PROGRESS

The baseline tests are now present. Refactoring can proceed with less risk.

Completed in current slice:

```text
split src/doctor.rs into grouped host/container/path/runtime/output modules
kept public doctor behavior unchanged
avoided mixing refactors with security hardening
```

Remaining targets:

```text
split src/ssh.rs into discovery/runtime/status/ui modules
keep public behavior unchanged while refactoring
```

Acceptance criteria:

```text
no CLI behavior changes
all tests pass
module boundaries are clearer
large unrelated functions are reduced
future M9/M10 work becomes easier
```

### 4. Clean up CLI parsing without expanding the command surface

**Status:** TODO

The manual parsing supports Pi pass-through ergonomics, but it should stay well tested and minimal.

Preserve the current command surface:

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

Tasks:

```text
keep tests for ambiguous cases
avoid adding new commands unless a milestone requires them
make help text and parsing behavior match exactly
consider a small parser helper type if it improves readability
```

Acceptance criteria:

```text
vr defaults to Pi
Pi argument pass-through remains stable
workspace parsing remains stable
help output remains accurate
```

### 5. M9 - Runtime hardening

**Status:** TODO

Improve security posture without breaking current Pi, SSH, Git, and login flows.

Scope:

```text
move away from container-root runtime where possible
reduce container capabilities
review host networking
explore Docker bridge networking again after rootless issues are understood
add safer mount policy
add read-only workspace option
add dangerous path warnings or prompts
block obviously dangerous workspace paths
add clearer trust-boundary docs
```

Constraints:

```text
do not break Pi login persistence
do not break SSH agent forwarding
do not break Git-over-SSH
do not reintroduce the M1 bind-mount failures
do not switch networking models without a proof
do not describe the result as a hardened sandbox until it is actually hardened
```

Candidate staged implementation:

```text
add opt-in capability drop / no-new-privileges settings
test non-root container runtime with the final mount model
add UID/GID mapping options if needed
add optional read-only workspace mode
consider tmpfs for /tmp and other scratch paths
review read-only root filesystem feasibility
prove bridge networking before changing defaults
make proven safe hardening defaults only after validation
```

Acceptance criteria:

```text
Pi still launches
Pi login still persists
Git-over-SSH still works
vr shell still works
the container no longer needs root, or the reason root remains is documented
host networking is reduced or the reason it remains is documented
risky mount paths are warned, prompted, or blocked
security docs are updated honestly
```

### 6. Improve workspace mount policy

**Status:** TODO

Current workspace safety checks are useful but should become stricter and more explicit.

Tasks:

```text
review symlinked workspace handling
review symlinked Vegasroom state paths
add stronger warnings or prompts for broad host mounts
consider policy config for risky mounts: warn, prompt, deny
add read-only workspace option as part of M9
```

Acceptance criteria:

```text
credential directories remain blocked
/ and virtual system roots remain blocked
symlink behavior is documented and tested
broad host mounts are deliberate, not accidental
```

### 7. Prepare for M10 with a small harness descriptor

**Status:** TODO

Do this before adding a second harness. The goal is to make Pi use a small internal descriptor without creating a plugin system.

Tasks:

```text
identify minimal harness fields: service name, image, command, state dirs, Dockerfile path
adapt Pi code to use that descriptor
keep Compose/runtime model unchanged
avoid marketplace/plugin abstractions
```

Acceptance criteria:

```text
Pi behavior is unchanged
harness-specific paths are isolated behind a small descriptor
a second harness can be added with less duplication
```

### 8. Documentation consolidation

**Status:** TODO

Keep documentation accurate and reduce duplicated planning material.

Tasks:

```text
keep README concise
move detailed behavior into docs/
keep TODO.md as canonical roadmap
remove stale claims when implementation changes
update docs/security.md after every M9 hardening change
update docs/config.md whenever config semantics change
```

Acceptance criteria:

```text
README, docs, and implementation agree
TODO statuses are explicit
out-of-scope work remains clearly separated at the bottom of this file
```

---

## Completed work

### MVP M1-M5 - Proven Pi runtime

**Status:** DONE

Completed capabilities:

```text
source-built Rust CLI wrapper
managed Vegasroom state directory
managed Compose runtime materialized by vr init
local Pi image build through vr init --build
vr doctor readiness checks
vr pi launches Pi
vr shell launches debug shell
rootless Docker context usage
SSH agent forwarding when host SSH_AUTH_SOCK is usable
Pi login persistence through mounted Pi state
```

### M6 - Workspace and CLI ergonomics

**Status:** DONE

Completed capabilities:

```text
vr defaults to vr pi
vr pi [workspace] [pi-args...]
vr shell [workspace]
default workspace support
current-directory workspace support
named managed workspace support
relative path workspace support
tilde path expansion
absolute path support when path exists
Pi argument pass-through
workspace refusal rules for dangerous credential/system paths
workspace docs
```

### Managed SSH single-key/simple-key workflow

**Status:** DONE

Completed capabilities:

```text
vr ssh configure
vr ssh status
recursive key discovery
interactive TUI selection
line-mode fallback
temporary managed ssh-agent lifecycle
selected key fingerprint checks
no private key copying into the container
no host ~/.ssh mount into the container
```

Note: repo-specific deploy-key routing is not part of this completed work. It is tracked as M7 and is out of scope for now.

### Build/test baseline

**Status:** DONE

Completed capabilities:

```text
manual GitHub Actions workflow
local scripts/check.sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features
cargo test --locked --all-targets --all-features
baseline unit tests for CLI/config/workspace/SSH helpers
removed unused direct thiserror dependency
```

### Git identity runtime behavior

**Status:** DONE

Completed capabilities:

```text
git.user_name and git.user_email are honored when configured
exactly one selected SSH key with git_user_name/git_user_email can provide identity
host global Git config is inherited only when git.inherit_host is true
Git identity is injected into the room through a per-launch generated Compose override
GIT_AUTHOR_NAME, GIT_AUTHOR_EMAIL, GIT_COMMITTER_NAME, and GIT_COMMITTER_EMAIL are set
GIT_CONFIG_GLOBAL points to a generated read-only gitconfig inside the room
vr doctor reports the effective host-side and room-side Git identity
Git identity precedence has unit coverage
```

### Per-launch runtime cache files

**Status:** DONE

Completed capabilities:

```text
per-launch runtime directories are created under ~/.vegasroom/cache
SSH agent Compose overrides are written per invocation
Git identity Compose overrides and generated gitconfig files are written per invocation
generated runtime files are held by an RAII guard while Docker Compose runs
generated runtime directories are removed on normal exit on a best-effort basis
concurrent vr pi / vr shell sessions no longer share fixed override file paths
```

---

## Out of scope for now

The following milestones are deliberately out of scope for the current active work. Keep them at the bottom of this document until they are reactivated.

### M7 - Managed SSH and repo-specific Git access

**Status:** OUT OF SCOPE FOR NOW

Goal:

```text
Support multiple SSH keys while ensuring repo-specific deploy keys are used for the right repository.
```

Problem:

```text
SSH agents can hold multiple keys.
GitHub deploy keys are often repository-specific.
If the wrong key is offered first, auth can fail or select the wrong identity.
Vegasroom currently forwards an agent socket or managed temporary agent but does not manage per-repo SSH identity selection.
```

Constraints:

```text
do not copy private keys into the container
do not mount host ~/.ssh into the container
do not store private key material or passphrases
preserve managed temporary ssh-agent lifecycle
private keys remain on the host and are only loaded into the temporary managed agent
the room receives only the agent socket and generated non-secret SSH config/state
```

Possible future approaches:

```text
room-local SSH config with host aliases
per-repo Git core.sshCommand
repo-key helper commands
selected-key metadata for intended repo or host patterns
```

Acceptance criteria when reactivated:

```text
a user can configure multiple deploy keys
Git operations for repo A use repo A's deploy key
Git operations for repo B use repo B's deploy key
private keys remain on the host
private keys are only loaded into the temporary managed agent
the container receives only the agent socket and generated non-secret SSH config/state
Git-over-SSH still works for the simple single-key case
```

### M8 - Host bootstrap

**Status:** OUT OF SCOPE FOR NOW

Goal:

```text
Make Vegasroom easier to set up on a fresh Linux machine by detecting, installing, and configuring host dependencies where practical.
```

Current MVP behavior:

```text
The user must already have Docker, Docker Compose, and a usable rootless Docker context.
Vegasroom diagnoses missing pieces with vr doctor.
The user fixes Docker/rootless setup manually.
```

Possible future commands:

```bash
vr bootstrap
vr bootstrap --check
vr bootstrap --docker
vr bootstrap --rootless-docker
vr bootstrap --print-only
```

Bootstrap policy if reactivated:

```text
do not install Docker silently
do not run sudo without explicit confirmation
do not overwrite existing Docker configuration
do not replace a working Docker setup
do not assume one Linux distribution
prefer showing exact commands before running them
provide --print-only for copy/paste installation
```

Acceptance criteria when reactivated:

```text
vr bootstrap --check reports Docker/rootless readiness clearly
vr bootstrap --print-only prints exact install/setup commands for the detected system
vr bootstrap --rootless-docker can configure the rootless Docker context on at least one supported Linux family
vr doctor recognizes the resulting setup
vr init --build succeeds after bootstrap
vr launches Pi without the user manually constructing Docker commands
installing Docker is documented as a host modification
rootless Docker is documented as a user daemon, not a hardened sandbox
users can review commands before running them
```
