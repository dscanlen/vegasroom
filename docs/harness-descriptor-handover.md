# Harness descriptor handover

This document captures the current state of the harness-descriptor preparation work and the recommended next phases. It is intended as a compact handoff before adding Claude Code, Codex, or other future harnesses.

## Current branch

```text
harness-descriptor-prep
```

## Current status

Phase 1 has started. The code now has an internal Pi harness descriptor in:

```text
src/harness.rs
```

The descriptor is intentionally small and static. It is not a plugin system and it does not add a new user-facing harness yet.

Current descriptor fields:

```rust
pub struct HarnessDescriptor {
    pub id: &'static str,
    pub display_name: &'static str,
    pub service_name: &'static str,
    pub default_image: &'static str,
    pub default_command: &'static str,
    pub dockerfile_path: &'static str,
    pub container_home: &'static str,
    pub state_dirs: &'static [HarnessStateDir],
    pub auth_state_relative_path: &'static str,
}
```

Current Pi descriptor values preserve the existing runtime contract:

```text
id: pi
display_name: Pi
service_name: pi
default_image: vegasroom/pi:local
default_command: pi
dockerfile_path: harness/pi/Dockerfile
container_home: /home/agent
state dirs:
  config     -> /home/agent/.pi/agent
  extensions -> /home/agent/.pi/extensions
  skills     -> /home/agent/.pi/skills
  sessions   -> /home/agent/.pi/sessions
auth_state_relative_path: config/auth.json
```

## Behavior expectation

No behavior change is intended in this phase.

These should behave exactly as before:

```bash
vr init
vr init --build
vr doctor
vr pi
vr shell
```

The config shape remains unchanged. The Compose service remains `pi`. The default harness remains Pi.

## Files changed in Phase 1

```text
src/harness.rs
src/main.rs
src/config.rs
src/paths.rs
src/docker.rs
src/doctor/mod.rs
src/doctor/runtime.rs
TODO.md
```

## What Phase 1 already wires through the descriptor

- Pi default image in config defaults
- Pi default command in config defaults
- Pi state path names under `~/.vegasroom/harness/pi`
- Pi auth state relative path
- runtime Pi Dockerfile path
- Compose service name for build/run/shell/check invocations
- doctor Dockerfile checks
- disclaimer display name

## Validation already requested

Run:

```bash
bash scripts/check.sh
```

Then smoke test:

```bash
cargo run -- init
cargo run -- doctor
cargo run -- shell .
```

Inside shell:

```sh
pwd
ls -la /workspace
```

Optional:

```bash
cargo run -- pi -- --help
```

## Recommended next phase: finish descriptor adoption for Pi constants

Continue replacing hardcoded Pi container paths in doctor/runtime checks with descriptor-derived values where it improves clarity without creating awkward code.

Candidate areas:

```text
src/docker.rs
src/doctor/container.rs
src/doctor/mod.rs
src/doctor/path_checks.rs
```

Examples still worth reviewing:

```text
/home/agent/.pi/agent
/home/agent/.pi/sessions
Pi auth state labels/messages
Pi harness labels/messages
```

Keep this as a refactor only. Do not add Claude/Codex yet.

## Recommended next phase: descriptor-aware internal helpers

Introduce internal helper functions that accept a descriptor while keeping public CLI behavior unchanged. Examples:

```rust
build_harness_image(config, &harness::PI)
run_harness_command(config, &harness::PI, workspace, args)
ensure_harness_image_exists(config, &harness::PI)
```

Existing Pi-specific wrappers can remain:

```rust
build_pi_image(...)
run_pi(...)
ensure_pi_image_exists(...)
```

Those wrappers should delegate to descriptor-aware internals. This keeps the command surface stable while reducing future duplication.

## Recommended next phase: decide minimal multi-harness config shape

Before adding Claude Code, decide how to represent harness-specific config without moving too much shared runtime config.

Recommended near-term shape:

```yaml
harness:
  pi:
    image: vegasroom/pi:local
    command: pi
    network: host
    build_network: host
    read_only_workspace: false
    read_only_rootfs: false
  claude:
    image: vegasroom/claude:local
    command: claude
  codex:
    image: vegasroom/codex:local
    command: codex
```

For now, keep shared Docker/runtime hardening fields on Pi until a second harness proves what should be shared. Avoid a large config migration before it is necessary.

Potential future shared runtime section, but not recommended in the immediate next step:

```yaml
runtime:
  network: host
  build_network: host
  read_only_workspace: false
  read_only_rootfs: false
```

## Claude Code addition expectations

When adding Claude Code later, expect differences in:

```text
package installation
runtime command
auth/config state paths
cache paths
session paths
doctor checks
login behavior
```

Expected shared behavior:

```text
workspace resolution
SSH agent forwarding
managed SSH
Git identity injection
read-only workspace option
read-only rootfs option
risky workspace policy
rootless Docker context
per-launch runtime override files
```

## Codex addition expectations

Codex should follow after Claude Code using the same descriptor pattern. Do not add Codex until the descriptor proves it handles Claude without major redesign.

## Scope guard

Do not build a marketplace/plugin system yet.

Avoid:

```text
dynamic third-party harness loading
runtime discovery of arbitrary harnesses
large CLI expansion before one additional harness is proven
large config migration before Claude validates the shape
```

Prefer:

```text
small static descriptors
clear internal helper functions
Pi behavior unchanged
one harness addition at a time
```

## Suggested acceptance criteria for descriptor prep

```text
Pi behavior unchanged
all tests pass
no CLI surface changes
Pi-specific paths/constants are isolated behind src/harness.rs or descriptor-aware helpers
a second static harness can be added with minimal duplicate Docker/runtime code
```
