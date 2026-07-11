# Config TUI design

`vr config` is intended to be the single interactive configuration entry point for Vegasroom.

## Command surface

```bash
vr config
```

`vr config` opens an interactive TUI shell when stdin/stdout are terminals. It should not grow a large `vr config <subcommand>` tree. Configuration discovery, presets, submenus, save prompts, and advanced actions live inside the TUI.

If `vr config` is run without an interactive terminal, it should explain that configuration is interactive and point users to manual YAML editing at `~/.vegasroom/config.yaml`.

## Navigation model

Follow the shared Vegasroom TUI interaction pattern:

```text
↑/↓ or k/j  move
Enter       open/select/toggle depending on screen
s           save changes
q           quit
```

Save, discard, and exit should not be top-level menu entries. They are actions/keybindings. If there are unsaved changes and the user quits, show a dirty-state prompt:

```text
Save changes before quitting? y/n/c
```

Nested submenus use `Esc` or Backspace to return to the previous screen, while `s` and `q` remain global actions.

## Top-level sections

The top-level TUI is intentionally small:

```text
Security
SSH
Advanced
```

Keep the menu stable and minimal. Avoid passive preview panes that change height as the highlight moves. Deeper or less-common settings belong in Advanced or manual YAML editing.

## Security presets

Security preset rows open a change preview first. Press Enter on the preview to apply the preset to the in-memory config, then press `s` to save it to disk.

User-facing preset names should prioritize clarity:

```text
Default / Compatible
Safer
Strict
Custom
```

Internal/documentation aliases may map to:

```text
lowsec
sec
highsec
custom
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

### SSH

The SSH menu item opens the managed SSH key picker directly. It reuses the same bottom-aligned visual language as the config menu and keeps the compact metadata for the highlighted key.

SSH-specific public commands are not part of the CLI. Use `vr config` for SSH key selection and `vr doctor` for SSH readiness/status checks.

### Advanced

Advanced contains less-common config/status actions and fields, including Git identity, color mode, config path, validation, backup information, and reset-to-defaults preview.

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

Initial editable controls toggle `git.inherit_host`. Editing `git.user_name` and `git.user_email` should use a later text-input flow.

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

The Output / color section cycles `ui.color` through `auto`, `always`, and `never`. A non-empty `NO_COLOR` environment variable remains an override that disables colored PASS/WARN/FAIL labels.

#### Other advanced actions

Expose non-everyday actions inside the TUI:

```text
config path
manual YAML editing instructions
validate current config
backup details
reset all to defaults with preview
```

## Save behavior

Before writing changes:

1. Show a summary of changed fields.
2. Create a timestamped backup of `config.yaml`.
3. Save the new config.
4. Reload/validate the saved config.
5. Show the backup path and save result.

Manual YAML editing remains supported and should be mentioned in the Advanced section.

## Implementation history

The first full Config TUI implementation exposed many sections. The modernized direction intentionally reduces the visible menu surface to Security, SSH, and Advanced while preserving the underlying config behavior and manual YAML editing support.
