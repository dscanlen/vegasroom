# TODO

- Ensure all TUI menus function consistently across Vegasroom, including alignment, bottom-panel rendering, visual styling, navigation keys, save/quit behavior, dirty-state prompts, and help/hotkey wording.
- Split `src/config_ui.rs` into smaller modules, likely state, render, actions, presets, diff/save helpers, and tests.
- Decide and implement release workflow details: tag strategy, release artifact targets, tag-only vs `workflow_dispatch`, changelog format, PR/push CI policy, and whether release automation should update pinned harness package versions using `scripts/update-pi-harness-version.sh latest`.
- Decide whether pinned harness package updates should be checked on a schedule, during release preparation, or only manually.
- Make config/runtime writes crash-safe by writing to a temporary file, syncing where appropriate, and atomically renaming over the destination.
- Revisit harness-independent package/library selection version syntax and update policy for `environment.apt.packages`, `environment.rust`, `environment.python`, `environment.go`, and `environment.typescript`.
- Add room-start stale environment image detection/warning: if current package/toolchain config differs from the generated derived image inputs, warn on `vr pi` / `vr shell` that the environment image is out of date and recommend `vr init --build` when ready.
- Improve `vr config` Environment polish: show package-cache size estimates before purge, improve confirmation wording if needed, and keep future toolchain activation controls in `vr config` as part of each toolchain feature.
- Brainstorm dependency-cache/profile concepts before implementation: keep toolchains minimal by default, preserve isolation, avoid global project dependency leakage, and consider whether named environment cache profiles/purge controls are worth adding later. The motivating idea is to persist package/module download caches for codebases across ephemeral containers so agents do not repeatedly download Cargo crates, Go modules, npm packages, or Python wheels between sessions. Any future layer needs explicit security controls to preserve Vegasroom's isolation policy, likely opt-in scoping, clear cache ownership, inspection, and purge controls; more design work is required before implementation.
- Revisit whether config TUI needs text-input editing for fields such as `paths.workspace`, `git.user_name`, and `git.user_email`, or whether manual YAML editing is sufficient.
- Add `default_harness` / bare `vr` harness selection once multiple harnesses exist.
- Decide whether a CLI `--color auto|always|never` override is needed in addition to persisted `ui.color` and `NO_COLOR`.
