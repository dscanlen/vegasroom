# Current work handover

This document captures the active feature/refactor sequence so work can resume cleanly after context switches.

## Branch

```text
feature/code-review-recommendations
```

This branch is for repo review and cleanup work. Keep changes incremental and behavior-preserving unless explicitly agreed otherwise.

## Completed and merged before this branch

### 1. Colored status labels

Merged to `main` in PR #14.

Completed:

```text
added bold colored PASS/WARN/FAIL labels
colored only the status word, not the whole line
applied to doctor output, workspace warnings/failures, SSH status/configure warnings, and related alerts
```

### 2. Consistent help formatting

Merged to `main` after the colored-label work.

Completed:

```text
standardized examples/notes across top-level and subcommand help
kept CLI behavior unchanged
added/updated help text tests
```

## Set aside for later decision

### Release workflow

This was intentionally paused.

Original idea:

```text
add a GitHub Actions release workflow
follow Cargo.toml package version
build a release binary
produce a concise changelog since the previous release/tag
```

Decision still needed:

```text
tag strategy
release artifact targets
whether to publish only on tags or also manually through workflow_dispatch
changelog format
```

## Active work: code review cleanup

Goal: improve modularity, consistency, and readability while preserving behavior.

### Subsection 1: split Docker runtime file helper

Committed and pushed on this branch:

```text
adbfbda Split Docker runtime files helper
```

Completed:

```text
moved RuntimeFiles and per-launch runtime directory allocation/cleanup from src/docker.rs to src/docker/runtime_files.rs
kept behavior unchanged
reduced src/docker.rs size
```

### Subsection 2: split SSH configure UI modules

Committed and pushed on this branch:

```text
5ba68ee Split SSH configure UI modules
```

Completed:

```text
moved src/ssh/ui.rs to src/ssh/ui/mod.rs
split line-mode configure UI into src/ssh/ui/line_mode.rs
split TUI rendering/wrapping/truncation into src/ssh/ui/render.rs
kept ConfigureUiState and persistence/matching helpers in src/ssh/ui/mod.rs
kept behavior unchanged
```

### Subsection 3: replace stringly typed selected SSH key checks

Committed and pushed on this branch:

```text
8ed69a7 Type selected SSH key check statuses
```

Completed:

```text
added SelectedKeyCheckStatus::{Pass, Warn, Fail}
added SelectedKeyCheck { status, detail }
changed ssh::selected_key_checks(config) to return typed checks instead of PASS:/WARN:/FAIL: strings
changed doctor host checks to map typed selected-key statuses directly
removed fragile prefix parsing from doctor host checks
removed unused SelectedKeyCheck re-export from src/ssh.rs after compiler warning
```

### Subsection 4: split Docker Git identity helper

Committed and pushed on this branch:

```text
daea139 Split Docker Git identity helper
```

Completed:

```text
moved GitIdentity and Git identity resolution/injection helpers from src/docker.rs to src/docker/git_identity.rs
kept public docker::effective_git_identity and docker::GitIdentity API stable through re-export
changed Compose launch assembly to call git_identity::prepare_override internally
moved Git identity unit tests into src/docker/git_identity.rs
```

### Subsection 5: split Docker doctor probes

Committed and pushed on this branch:

```text
1d341cc Split Docker doctor probes
```

Completed:

```text
moved SshAddCheck, ContainerDoctorProbe, and ContainerSshDoctorProbe from src/docker.rs to src/docker/doctor_probe.rs
moved container_doctor_probe and container_ssh_doctor_probe to src/docker/doctor_probe.rs
moved structured doctor probe output parsing helpers and tests to src/docker/doctor_probe.rs
kept public docker::container_doctor_probe, docker::container_ssh_doctor_probe, and currently referenced probe type APIs stable through re-export
```

### Subsection 6: split Docker Compose override helpers

Committed and pushed on this branch:

```text
f77733e Split Docker Compose override helpers
```

Completed:

```text
moved read-only-rootfs Compose override writer from src/docker.rs to src/docker/overrides.rs
changed Compose launch assembly to call overrides::prepare_read_only_rootfs internally
moved read-only-rootfs override unit test into src/docker/overrides.rs
```

Docker cleanup is complete for the originally recommended slices. Keep public Docker API stable while moving any future internals.

### Subsection 7: split CLI help text

Committed locally on this branch:

```text
452eb83 Split CLI help text
```

Completed:

```text
added src/cli/help.rs
moved top-level/subcommand after_help constants into cli::help
moved manual Pi and shell help text/print helpers into cli::help
moved Pi and shell help text unit tests into cli::help
kept CLI behavior unchanged
```

### Subsection 8: split CLI manual launch parser

Committed locally on this branch:

```text
4f70887 Split CLI manual launch parser
```

Completed:

```text
added src/cli/parser.rs
moved ManualLaunch and PiInvocation types into cli::parser
moved manual launch parsing helpers into cli::parser
moved manual parser unit tests into cli::parser
kept CLI behavior unchanged
```

Validated by user with:

```bash
./scripts/check.sh
```

### Subsection 9: split CLI command execution helpers

Committed locally on this branch:

```text
cf09f68 Split CLI command execution helpers
```

Completed:

```text
added src/cli/commands.rs
moved init, doctor, SSH status/configure, Pi launch, and shell launch execution helpers into cli::commands
kept CLI argument parsing and manual launch dispatch in src/cli.rs
kept CLI behavior unchanged
```

### Subsection 10: color behavior polish

Committed locally on this branch in this commit:

```text
Support NO_COLOR for status labels
```

Completed:

```text
added non-empty NO_COLOR support for PASS/WARN/FAIL status labels
kept colored status labels as the default when NO_COLOR is unset
added color-enabled and color-disabled status label tests without mutating process environment
```

Validated by user with:

```bash
./scripts/check.sh
```

Code-review cleanup subsections are complete. Remaining color policy polish is deferred to the future config update instead of continuing on this cleanup branch.

Deferred color/config items:

```text
non-TTY color auto-disable
possible --color auto|always|never config/flag
persisted color policy in future config flow, if needed
```

## Larger features still pending

### Config TUI and presets

Large feature, not started.

Desired direction:

```text
add vr config as a general configuration TUI
move SSH configure flow into vr config eventually
keep manual YAML editing supported
add security presets: lowsec default, sec, highsec
add default_harness for bare `vr` launches once multiple harnesses exist
bundle remaining color policy controls here if config-backed color behavior is desired
```

This should be split into design and several implementation branches.

### Harness-independent package/library selection

Large architectural feature, not started.

Desired direction:

```text
let users declare Rust/Python/npm/etc libraries for the room environment
keep the list independent of harness provider
avoid base image bloat by default
first document base image and required default packages
then decide build/generation model
```

Start with a design document before code.
