# Current work handover

This document captures the active feature/refactor sequence so work can resume cleanly after context switches.

## Branch

```text
feature/config-tui
```

This branch is for the interactive `vr config` TUI and preset work. Keep changes incremental and prefer design/skeleton slices before broad editing behavior.

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

## Active work: Config TUI and presets

Large feature, started on this branch.

Desired direction:

```text
add bare `vr config` as the single interactive configuration TUI entry point
avoid a broad `vr config <subcommand>` command tree
use TUI keybindings like existing `vr ssh configure`: s saves, q quits, dirty-state prompt on quit
configure all settings through top-level sections and nested submenus
keep manual YAML editing supported
add user-facing security presets: Default / Compatible, Safer, Strict
map presets to lowsec/sec/highsec-style behavior internally or in docs if useful
add default_harness for bare `vr` launches once multiple harnesses exist
bundle remaining color policy controls here if config-backed color behavior is desired
```

Design started:

```text
added docs/config-tui.md with command surface, navigation model, sections, presets, save behavior, and implementation slices
```

### Subsection 1: add config TUI shell

Committed locally on this branch:

```text
98bc60e Add config TUI shell
```

Completed and validated by user with `./scripts/check.sh`.

```text
added `vr config` command
added read-only config TUI shell with Overview, Security preset, Workspace, SSH, Git identity, Runtime / Docker, Output / color, and Advanced sections
added non-TTY fallback that points users to manual YAML config editing
added security preset detection helpers and tests
kept save/discard/exit out of the menu; save/quit are keybindings
```

### Subsection 2: add config section submenus

Committed locally on this branch:

```text
f2e0e52 Add config TUI section navigation
```

Completed and validated by user with `./scripts/check.sh`.

```text
added real top-level-to-section navigation in the config TUI
added per-section submenu rows for planned editable fields/actions
added Esc/Backspace navigation back from section screens while keeping s/q as global save/quit keybindings
kept submenus read-only placeholders for this slice
added a test that the Workspace section exposes expected config rows
```

### Subsection 3: add config save model and backups

Committed locally on this branch:

```text
a6d26bb Add config TUI save model
```

Completed and validated by user with `./scripts/check.sh`.

```text
changed the global s keybinding to call real save plumbing
added dirty-state quit prompt with save, discard, and cancel choices
updated config TUI design doc to include the cancel prompt option
added timestamped config backup writer before saving over an existing config
reloads and validates config after save
added tests for backup writing and dirty-state save clearing
```

### Subsection 4: add security preset editing

Committed locally on this branch in this commit:

```text
Add config TUI security presets
```

Completed and validated by user with `./scripts/check.sh`.

```text
added SecurityPreset::{DefaultCompatible, Safer, Strict}
changed Security preset submenu rows to open a change preview screen
Enter on the preview applies the selected preset to in-memory config and marks the TUI dirty only when values changed
preset application preserves host networking for all presets because bridge remains experimental for Pi login
added preset diff helpers for previewing exact field changes
added tests for Safer preview diff, Strict preset application, and matching-preset no-op dirty behavior
```

### Subsection 5: add workspace config editor

Committed locally on this branch:

```text
Add config TUI workspace editor
```

Completed and validated by user with `./scripts/check.sh`.

```text
made Workspace section rows for risky_mount_policy and read_only_workspace editable toggles
Enter on risky_mount_policy toggles warn/deny and marks config dirty
Enter on read_only_workspace toggles true/false and marks config dirty
kept paths.workspace as a placeholder for a later text-input slice
added workspace editor toggle tests
```

### Subsection 6: add runtime hardening editor

Committed locally on this branch:

```text
Add config TUI runtime hardening editor
```

Completed; validation was not run in this agent environment because cargo/rustfmt are unavailable.

```text
made Runtime / Docker read-only root filesystem row an editable toggle
Enter on read_only_rootfs toggles true/false and marks config dirty
kept runtime/build network fields as read-only placeholders because bridge remains experimental for Pi login
added runtime editor toggle tests
```

### Subsection 7: add ui.color config and output/color editor

Committed locally on this branch:

```text
Add config TUI color editor
```

Completed and validated by user with `./scripts/check.sh`.

```text
added ui.color config with auto/always/never values and default auto
changed colored status labels to honor ui.color while preserving non-empty NO_COLOR as an override
added Output / color editor row that cycles auto -> always -> never -> auto
updated config docs and config TUI design docs for active ui.color behavior
added config parsing, alert policy, and config TUI color editor tests
```

### Subsection 8: add SSH mode editor and key configure handoff

Committed locally on this branch:

```text
Add config TUI SSH editor
```

Completed and validated by user with `./scripts/check.sh`.

```text
made SSH mode row editable and cycle auto -> host -> managed -> off -> auto
made selected managed SSH keys row hand off to the existing vr ssh configure flow instead of duplicating key selection
blocks SSH key configure handoff while config TUI has unsaved changes so direct writes from the SSH flow do not clobber pending config edits
reloads config after returning from the SSH configure flow
updated config TUI design docs for active SSH behavior
added SSH mode, row exposure, dirty-blocking, and handoff action tests
```

### Subsection 9: add Git identity editor and effective preview

Committed locally on this branch:

```text
Add config TUI Git identity editor
```

Completed and validated by user with `./scripts/check.sh`.

```text
made Git identity inherit_host row editable and toggle true/false
added effective Git identity preview using existing Git identity precedence
kept git.user_name and git.user_email as placeholders for a later text-input slice
updated config TUI design docs for active Git identity behavior
added Git inherit_host toggle, effective preview, and row exposure tests
```

### Subsection 10: polish validation, reset actions, and advanced screen

Currently implemented locally and awaiting validation/commit:

```text
added Advanced validate-current-config action
updated Advanced backup wording to reflect existing timestamped backup behavior
added reset-all-to-defaults preview screen with exact changed-field diff
Enter on reset preview applies defaults in memory and marks config dirty only when values changed
updated config TUI design docs for active Advanced behavior
added Advanced row exposure, validation, reset preview, and reset application tests
```

Validation needed before commit:

```bash
./scripts/check.sh
```

Suggested commit message after validation:

```text
Polish config TUI advanced actions
```

After this validates, the planned Config TUI slices are complete and ready for end-to-end user testing.

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
