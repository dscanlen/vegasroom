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

Currently implemented locally and awaiting test/commit.

Completed locally:

```text
added SelectedKeyCheckStatus::{Pass, Warn, Fail}
added SelectedKeyCheck { status, detail }
changed ssh::selected_key_checks(config) to return typed checks instead of PASS:/WARN:/FAIL: strings
changed doctor host checks to map typed selected-key statuses directly
removed fragile prefix parsing from doctor host checks
```

Known current fix already applied:

```text
removed unused SelectedKeyCheck re-export from src/ssh.rs after compiler warning
```

Validation needed before commit:

```bash
./scripts/check.sh
```

Suggested commit message after validation:

```text
Type selected SSH key check statuses
```

## Remaining recommended code-review subsections

### 4. Continue splitting `src/docker.rs`

Recommended next slices, one at a time:

```text
move Git identity resolution/injection to src/docker/git_identity.rs
move doctor container probes to src/docker/doctor_probe.rs
move generated Compose override writers to src/docker/overrides.rs
keep public Docker API stable while moving internals
```

Do not combine all of these in one large commit.

### 5. Split CLI module later

`src/cli.rs` is readable but large. Consider later:

```text
src/cli.rs
src/cli/help.rs
src/cli/parser.rs
src/cli/commands.rs
```

Only do this after Docker cleanup, and preserve all parsing tests.

### 6. Color behavior polish, optional later

Current colors always emit ANSI. Future optional polish:

```text
NO_COLOR support
non-TTY auto-disable
possible --color auto|always|never config/flag
```

Do not do this unless prioritized.

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
