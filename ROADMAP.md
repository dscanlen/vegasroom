# Vegasroom Roadmap

Last updated: 2026-11-24

This file is the maintained roadmap; update it alongside the code. (It started as a Claude Doc snapshot, which is no longer kept in sync.)

## Where things stand

Milestones 1–3 are complete and verified on the real setup; the next milestone is M4, a built-in `vr update`.

| Milestone | Delivered | Commits |
| --- | --- | --- |
| M3 · Shared base image | One `vegasroom/base` image holds the OS tools and the environment profile; Pi and Claude Code are thin agent layers on it. Base fingerprint label, harness-on-base check, unused-image hints. | `6417c0e`…`be88075` |
| M2 · Claude Code harness | `vr claude [workspace]`, multi-service compose with `VR_<ID>_*` vars, per-harness state paths and doctor, persisted browser login, host `ANTHROPIC_API_KEY` / `CLAUDE_CODE_OAUTH_TOKEN` passthrough. Claude Code pinned at 2.1.325. | `b0313a9`…`9b9e796` |
| M1 · Generic harness foundation | `HarnessDescriptor` registry, generated compose, `StatePaths` threading, harness-agnostic CLI routing. | merged in `51ca13f` |

## Next: M4 · `vr update`

M4 replaces `scripts/update-harness.sh` and the manual Claude Code version bump with one command that resolves, pins, rebuilds and checks. The shared base makes an agent bump cheap: only the thin harness layer rebuilds.

**Command surface**

- `vr update` — update every registered harness
- `vr update pi` / `vr update claude` — one harness to latest
- `vr update pi 1.7.0` — pin an exact version
- `vr update --check` — report available updates, change nothing

**Version resolution.** Both agents ship on npm: `@earendil-works/pi-coding-agent` and `@anthropic-ai/claude-code`. Each descriptor names its package and pinned `ARG` (`PI_CODING_AGENT_VERSION`, `CLAUDE_CODE_VERSION`). Claude Code publishes a `stable` dist-tag; decide whether `vr update claude` follows `stable` or `latest` by default (current lean: `stable`).

**Stage runner** (named stages, progress, stop on first failure):

1. Prerequisites: Docker context reachable, npm available
2. Resolve and validate the version
3. Pin the Dockerfile `ARG`
4. Repository checks and `cargo install` in a dev checkout; skipped for binary installs
5. Rebuild: base from cache, then the harness layer
6. Harness package sync (Pi: same version into the persisted npm-global prefix, then `pi update --extensions`)
7. `vr doctor`

**Rollback (stretch).** Snapshot the Dockerfile pin and config before updating; on failure offer to restore, or at minimum print exact recovery steps.

**Launch version check (optional).** Non-blocking background check on `vr pi` / `vr claude` that prints one line when an update exists; respect `PI_SKIP_VERSION_CHECK` and a config flag.

**Migration.** Keep `update-harness.sh` for a release or two after `vr update pi` is stable, then move its contract tests (`scripts/test-update-harness.sh`) into Rust unit tests.

**Design questions to settle when planning M4**

- Pinned versions live in embedded Dockerfiles, so an end-user binary cannot rewrite them. Does `vr update` write a version override into config (passed as a build arg) instead of editing the Dockerfile?
- Should `vr update` ever rebuild the `vr` binary itself, or only images and packages? Current lean: binary only in a dev checkout.

## Stretch: named environment profiles

Allow several named toolchain profiles instead of one, each built as its own base image; today one shared profile covers every workspace.

- Config shape, for example `environments: { rust: {…}, data: {…} }`, with the current `environment:` block as the default profile.
- One base image per profile (`vegasroom/base:<profile>`); harness images are built per profile through the existing `BASE_IMAGE` build arg.
- Selection per launch (`vr claude . --env data`) or per workspace (a marker file in the workspace).
- Open: how many harness × profile images to build eagerly versus on first use, and how unused-image hints treat profiles no workspace selects.

## Smaller follow-ups

None of these block current use; each is a self-contained change that can ride along with M4 or stand alone.

| Item | Why | Size |
| --- | --- | --- |
| `vr config` UI for `harness.claude` and `environment.base_image` | The TUI and its presets only edit `harness.pi`; Claude Code settings need hand-edited YAML. | Medium |
| Rust toolchain check in `vr doctor` | Container probes cover Python, Go and TypeScript, not Rust, although Rust is in the profile. | Small |
| `environment.build_network` for the base build | The base build borrows the default harness's `build_network`; a shared image should have its own setting. | Small |
| `IS_SANDBOX=1` for Claude Code | Would allow `--dangerously-skip-permissions` in the room (container root maps to the host user). Deliberately left out; verify the variable against the installed version first. | Small |
| Configurable default harness (`default_harness: pi` or `claude`) | `vr` and top-level flags always launch Pi. | Small |
| `vr shell` per harness | `vr shell` always uses the Pi image. | Small |
| Thread `StatePaths` through remaining calls | `docker.rs` probes and `Config::resolved_compose_file` still call `StatePaths::default()`, which makes them hard to test against a temp state. | Small |

## Open design questions

Three questions from M2 planning remain open; none blocks M4.

- **MCP servers.** Claude Code's MCP config is currently managed inside the room, persisted in `~/.vegasroom/harness/claude/config`. Should Vegasroom also offer mounting selected host MCP config, or keep the room fully separate from the host?
- **Network patterns.** Both harnesses run with `host` networking by default. Claude Code's browser login uses a paste-back code, so it also works on `bridge`; is a stricter default worth testing per harness?
- **Extensions and plugins.** Pi has its own extensions/skills dirs and npm-global updates; Claude Code keeps plugins and skills in its config dir. Is there a shared model worth exposing in `vr`, or do they stay harness-specific?

## Housekeeping

Loose ends from the M3 close-out; decisions rather than code.

- [ ] **Push `main`.** At close-out, local `main` was 23 commits ahead of `origin/main`: M1 (including merge `51ca13f`), M2 (9 commits) and M3 (5 commits).
- [ ] **Remove pre-M3 images** once the new setup has proven itself: `docker --context rootless image rm vegasroom/claude:latest-env vegasroom/pi:local vegasroom/pi:local-env`
- [ ] **Bump the crate version.** `Cargo.toml` is still 0.7.0 after M2 and M3; versioned image tags (`vegasroom/pi:0.7.0`) now point at the new thin images. 0.8.0 would mark the change.
