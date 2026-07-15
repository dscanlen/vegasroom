# TODO

## Branch and commit workflow

- Create one branch per feature/fix/change before editing code. Use focused names such as `fix/digest-derived-image-tag` or `feature/private-state-permissions`.
- Commit after each code change, grouped by file or tightly related file set, with a useful message.
- Do not push a feature branch until the feature/fix is working as expected and checks pass.
- Run `bash scripts/check.sh` before handoff/merge unless the change is documentation-only.

1. Revisit harness-independent package/library selection version syntax and update policy.
    - Cover `environment.apt.packages`, `environment.rust`, `environment.python`, `environment.go`, and `environment.typescript`.

2. Brainstorm dependency-cache/profile concepts before implementation.
    - Keep toolchains minimal by default.
    - Preserve isolation and avoid global project dependency leakage.
    - Consider whether named environment cache profiles/purge controls are worth adding later.
    - Motivation: persist package/module download caches for codebases across ephemeral containers so agents do not repeatedly download Cargo crates, Go modules, npm packages, or Python wheels between sessions.
    - Any future layer needs explicit security controls, likely opt-in scoping, clear cache ownership, inspection, and purge controls.

## P3 - release/process/future features

3. Decide and implement release workflow details.
    - Tag strategy, release artifact targets, tag-only vs `workflow_dispatch`, changelog format, PR/push CI policy, and whether release automation should update pinned harness package versions using `scripts/update-pi-harness-version.sh latest`.

4. Decide whether pinned harness package updates should be checked on a schedule, during release preparation, or only manually.

5. Add `default_harness` / bare `vr` harness selection once multiple harnesses exist.

6. Decide whether a CLI `--color auto|always|never` override is needed in addition to persisted `ui.color` and `NO_COLOR`.

7. Revisit `vr ssh` command handling.
    - The manual parser reserves `ssh`, but no public `vr ssh` command exists.
    - Decide whether to add a command, route to `vr config`, or stop reserving it.
