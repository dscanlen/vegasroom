# TODO

- Ensure all TUI menus function consistently across Vegasroom, including alignment, bottom-panel rendering, visual styling, navigation keys, save/quit behavior, dirty-state prompts, and help/hotkey wording.
- Split `src/config_ui.rs` into smaller modules, likely state, render, actions, presets, diff/save helpers, and tests.
- Decide and implement release workflow details: tag strategy, release artifact targets, tag-only vs `workflow_dispatch`, changelog format, PR/push CI policy, and whether release automation should update pinned harness package versions using `scripts/update-pi-harness-version.sh latest`.
- Decide whether pinned harness package updates should be checked on a schedule, during release preparation, or only manually.
- Make config/runtime writes crash-safe by writing to a temporary file, syncing where appropriate, and atomically renaming over the destination.
- Implement harness-independent package/library selection from `docs/package-selection.md`; start with `environment.apt.packages` and a generated derived image, then decide language package manager support.
- Revisit whether config TUI needs text-input editing for fields such as `paths.workspace`, `git.user_name`, and `git.user_email`, or whether manual YAML editing is sufficient.
- Add `default_harness` / bare `vr` harness selection once multiple harnesses exist.
- Decide whether a CLI `--color auto|always|never` override is needed in addition to persisted `ui.color` and `NO_COLOR`.
