# TODO

## Branch and commit workflow

- Create one branch per feature/fix/change before editing code. Use focused names such as `fix/digest-derived-image-tag` or `feature/private-state-permissions`.
- Commit after each code change, grouped by file or tightly related file set, with a useful message.
- Do not push a feature branch until the feature/fix is working as expected and checks pass.
- Run `bash scripts/check.sh` before handoff/merge unless the change is documentation-only.

## Completed

- Fixed derived image tag generation for digest-based base images.
- Updated the pinned Pi harness package to 0.80.7.
- Implemented standard Pi harness image tags: `vegasroom/pi:latest` plus `vegasroom/pi:<vr-version>`.

## P0 - next fixes

1. Harden permissions for sensitive Vegasroom state.
   - Set `~/.vegasroom` and sensitive subdirectories to private Unix permissions where supported.
   - Sensitive areas include Pi auth/config, sessions, npm-global persisted executables, SSH state, cache runtime overrides, and generated gitconfig/agent override files.
   - Add `vr doctor` permission checks and remediation guidance.

2. Add semantic config validation.
   - Keep serde round-trip validation, but also validate meaningful config values before save/launch/doctor.
   - Include environment apt/npm/Rust validation, Docker image/network sanity, SSH mode constraints, and workspace/security policy sanity.
   - Consider warning on unknown YAML fields so typos in security-sensitive config are not silently ignored.

3. Improve config TUI information display.
   - Render `SectionRow` details/current values instead of hiding them in `_details`.
   - Preserve stable bottom-panel layout; avoid jumpy passive preview panes.
   - Add tests for rendered output containing current values such as Git identity, color mode, toolchain enabled state, and cache purge explanation.

## P1 - security and consistency polish

4. Make config/runtime writes crash-safe.
   - Write to a temporary file in the same directory, flush/sync where appropriate, and atomically rename over the destination.
   - Apply to `Config::save_to_path`, managed runtime writes, generated overrides, and config TUI backup/save flow.

5. Ensure all TUI menus function consistently across Vegasroom.
   - Align bottom-panel rendering, visual styling, navigation keys, save/quit behavior, dirty-state prompts, and help/hotkey wording.
   - Reconcile config TUI and SSH key picker behavior, including Enter/Space handling, Esc/Backspace semantics, and status/notice wording.
   - Respect `ui.color` and `NO_COLOR` consistently in TUI and line-mode output.

6. Update `docs/config-tui.md` to match current behavior or adjust the TUI back to the documented design.
   - Current code exposes top-level `Security`, `Environment`, `SSH`, and `Advanced`.
   - Existing doc still says top-level sections are `Security`, `SSH`, and `Advanced`.
   - Document whether Environment remains top-level.
   - Document current save behavior or implement the planned changed-field summary before save.

7. Resolve custom Compose-file behavior.
   - `docker.compose_file` appears configurable, but launch/init currently repairs it back to the managed runtime file.
   - Decide whether custom Compose files are supported.
   - If unsupported, remove/deprecate the field or document it as managed/internal.

8. Sanitize generated YAML values consistently.
    - SSH agent override path escaping should replace CR/LF like Git identity YAML escaping does.
    - Add tests for newline-containing hostile `SSH_AUTH_SOCK` values.

## P2 - planned refactors and UX improvements

9. Split `src/config_ui.rs` into smaller modules without changing behavior.
    - Preserve the current TUI flow, keybindings, rendering, dirty/save/quit semantics, confirmation prompts, and tests.
    - Suggested layout: `src/config_ui/mod.rs` as the orchestration/public entrypoint, `state.rs` for `ConfigUiState`/screens/actions, `render.rs` for drawing/truncation/bottom panel, `sections.rs` for section/row definitions, `presets.rs` for security presets/diffs, `persistence.rs` for save/backup/reset helpers, and `cache.rs` for environment cache purge helpers.
    - Move colocated tests with the code they cover where practical.
    - Do this in small mechanical slices and run `bash scripts/check.sh` after each slice.

10. Revisit config TUI text-input editing.
    - Decide whether fields such as `paths.workspace`, `git.user_name`, and `git.user_email` need in-TUI text input or whether manual YAML editing is sufficient.

11. Improve `vr config` Environment polish.
    - Show package-cache size estimates before purge.
    - Improve confirmation wording if needed.
    - Keep future toolchain activation controls in `vr config` as part of each toolchain feature.

12. Revisit harness-independent package/library selection version syntax and update policy.
    - Cover `environment.apt.packages`, `environment.rust`, `environment.python`, `environment.go`, and `environment.typescript`.

13. Brainstorm dependency-cache/profile concepts before implementation.
    - Keep toolchains minimal by default.
    - Preserve isolation and avoid global project dependency leakage.
    - Consider whether named environment cache profiles/purge controls are worth adding later.
    - Motivation: persist package/module download caches for codebases across ephemeral containers so agents do not repeatedly download Cargo crates, Go modules, npm packages, or Python wheels between sessions.
    - Any future layer needs explicit security controls, likely opt-in scoping, clear cache ownership, inspection, and purge controls.

## P3 - release/process/future features

14. Decide and implement release workflow details.
    - Tag strategy, release artifact targets, tag-only vs `workflow_dispatch`, changelog format, PR/push CI policy, and whether release automation should update pinned harness package versions using `scripts/update-pi-harness-version.sh latest`.

15. Decide whether pinned harness package updates should be checked on a schedule, during release preparation, or only manually.

16. Add `default_harness` / bare `vr` harness selection once multiple harnesses exist.

17. Decide whether a CLI `--color auto|always|never` override is needed in addition to persisted `ui.color` and `NO_COLOR`.

18. Revisit `vr ssh` command handling.
    - The manual parser reserves `ssh`, but no public `vr ssh` command exists.
    - Decide whether to add a command, route to `vr config`, or stop reserving it.
