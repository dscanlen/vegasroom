# TODO

- Ensure all TUI menus function consistently across Vegasroom, including alignment, bottom-panel rendering, visual styling, navigation keys, save/quit behavior, dirty-state prompts, and help/hotkey wording.
- Decide and implement release workflow details: tag strategy, release artifact targets, tag-only vs `workflow_dispatch`, and changelog format.
- Implement harness-independent package/library selection from `docs/package-selection.md`; start with `environment.apt.packages` and a generated derived image, then decide language package manager support.
- Revisit whether config TUI needs text-input editing for fields such as `paths.workspace`, `git.user_name`, and `git.user_email`, or whether manual YAML editing is sufficient.
- Add `default_harness` / bare `vr` harness selection once multiple harnesses exist.
- Decide whether a CLI `--color auto|always|never` override is needed in addition to persisted `ui.color` and `NO_COLOR`.
