# Config TUI design

`vr config` is the single interactive configuration entry point for Vegasroom.

## Command surface

```bash
vr config
```

`vr config` opens an interactive TUI shell when stdin/stdout are terminals. It should not grow a large `vr config <subcommand>` tree. Configuration discovery, presets, submenus, save prompts, and advanced actions live inside the TUI.

If `vr config` is run without an interactive terminal, it explains that configuration is interactive and points users to manual YAML editing at `~/.vegasroom/config.yaml`.

## Navigation model

Follow the shared Vegasroom TUI interaction pattern:

```text
↑/↓ or k/j  move
Enter       open/select/toggle depending on screen
s           save changes
q           quit
```

Save, discard, and exit should not be top-level menu entries. They are actions/keybindings. Quitting always opens a confirmation prompt. If there are unsaved changes, ask whether to save before quitting:

```text
Save changes before quitting? y/n/c
```

If there are no unsaved changes, ask for confirmation before exiting:

```text
No unsaved changes. Quit? y/n
```

Nested config screens use `Esc` to return to the previous screen, while `s` and `q` remain global actions. Text input screens are the exception: printable keys edit the value, Enter applies it in memory, and Esc cancels back to the section. On the root config menu, `Esc` quits like `q`.

## Top-level sections

The current top-level TUI sections are:

```text
Security
Environment
SSH
Advanced
```

Keep the menu stable and minimal. Avoid passive preview panes that change height as the highlight moves. Less-common settings belong in Advanced or manual YAML editing. Simple text values may open an in-TUI editor; complex list/runtime settings should remain manual YAML edits.

## Security presets

Security preset rows open a change preview first. Press Enter on the preview to apply the preset to the in-memory config, then press `s` to save it to disk.

Current preset rows:

```text
Default / Compatible
Safer
Strict
```

### Default / Compatible

Preserves the current proven behavior:

```yaml
workspace.risky_mount_policy: warn
harness.pi.read_only_workspace: false
harness.pi.read_only_rootfs: false
harness.pi.network: host
harness.pi.build_network: host
ssh.mode: auto
git.inherit_host: true
```

### Safer

Improves accidental exposure protection without breaking common workflows:

```yaml
workspace.risky_mount_policy: deny
harness.pi.read_only_workspace: false
harness.pi.read_only_rootfs: false
harness.pi.network: host
harness.pi.build_network: host
ssh.mode: auto
git.inherit_host: true
```

### Strict

Security-forward settings with explicit compatibility warnings:

```yaml
workspace.risky_mount_policy: deny
harness.pi.read_only_workspace: true
harness.pi.read_only_rootfs: true
harness.pi.network: host
harness.pi.build_network: host
ssh.mode: managed
git.inherit_host: false
```

Do not move presets to bridge networking yet. Bridge remains experimental because Pi login currently depends on host-reachable localhost callback behavior.

## Section scope

### Environment

Environment is currently a top-level section because toolchain enablement and cache cleanup are common enough to expose directly.

Current rows:

```text
Rust
Python
Go
TypeScript
Purge package download caches
```

The toolchain rows show current enabled/disabled state and toggle in memory with Enter. Users must press `s` to save, then run `vr init --build` when ready to rebuild the environment image. Toolchain version strings and package lists are intentionally not shown or edited in the TUI; edit YAML for those deeper changes.

`Purge package download caches` opens a confirmation preview before deleting safe package download caches. The preview shows estimated removable size totals and per-cache path estimates. It removes npm/pip download caches plus Cargo registry/git caches, while preserving workspaces, auth, SSH, Pi npm-global installs, and Cargo-installed binaries.

### SSH

The SSH menu item opens the managed SSH key picker directly. It reuses the same bottom-aligned visual language as the config menu and keeps compact metadata for the highlighted key.

SSH-specific public commands are not part of the CLI. Use `vr config` for SSH key selection and `vr doctor` for SSH readiness/status checks.

### Advanced

Advanced contains less-common config/status actions and fields:

```text
Workspace path
Git: inherit host identity
Git: configured user.name
Git: configured user.email
Git: effective identity
Color mode
Config path
Validate current config
Recovery backup during save
Reset all to defaults
```

#### Git identity

Configure:

```text
git.inherit_host
git.user_name
git.user_email
```

The UI shows the effective identity preview based on current precedence:

```text
1. top-level git.user_name and git.user_email
2. exactly one selected SSH key with git identity metadata
3. host global Git config when git.inherit_host is true
```

Current editable controls toggle `git.inherit_host`. `paths.workspace`, `git.user_name`, and `git.user_email` open an in-TUI text editor. Press Enter to apply the typed value in memory, then press `s` to save it to disk. Leaving `git.user_name` or `git.user_email` blank clears that optional value; `paths.workspace` must not be blank.

Package lists, toolchain version strings, and runtime configuration remain manual-YAML-only because they are deeper changes with more validation and rebuild implications.

#### Output / color

Configure:

```text
ui.color
```

Supported values:

```text
auto    color terminal output only
always  force ANSI color
never   disable ANSI color
```

The Color mode row cycles `ui.color` through `auto`, `always`, and `never`. A non-empty `NO_COLOR` environment variable remains an override that disables ANSI color in TUI and line-mode output.

## Save behavior

Current save behavior is immediate when the user presses `s`; there is no changed-field summary screen yet.

When saving dirty config:

1. If `config.yaml` already exists, create a temporary timestamped recovery backup beside it.
2. Validate and atomically save the new config.
3. Reload/validate the saved config.
4. Delete the recovery backup after the save is confirmed valid.
5. Show the save result.

Recovery backups are not retained after successful saves. They are only left behind if saving, validation, or cleanup fails before the primary config is known-good.

Manual YAML editing remains supported at `~/.vegasroom/config.yaml`.

## Implementation history

The first full Config TUI implementation exposed many sections. The current interface keeps the top-level surface focused on Security, Environment, SSH, and Advanced while preserving the underlying config behavior and manual YAML editing support.
