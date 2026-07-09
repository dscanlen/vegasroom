# Config TUI design

`vr config` is intended to be the single interactive configuration entry point for Vegasroom.

## Command surface

```bash
vr config
```

`vr config` opens an interactive TUI shell when stdin/stdout are terminals. It should not grow a large `vr config <subcommand>` tree. Configuration discovery, presets, submenus, save prompts, and advanced actions live inside the TUI.

If `vr config` is run without an interactive terminal, it should explain that configuration is interactive and point users to manual YAML editing at `~/.vegasroom/config.yaml`.

## Navigation model

Follow the existing `vr ssh configure` interaction pattern:

```text
↑/↓ or k/j  move
Enter       open/select/toggle depending on screen
s           save changes
q           quit
```

Save, discard, and exit should not be top-level menu entries. They are actions/keybindings. If there are unsaved changes and the user quits, show a dirty-state prompt similar to `vr ssh configure`:

```text
Save changes before quitting? y/n/c
```

Nested submenus use `Esc` or Backspace to return to the previous screen, while `s` and `q` remain global actions.

## Top-level sections

The top-level TUI should expose sections, not commands:

```text
Overview
Security preset
Workspace
SSH
Git identity
Runtime / Docker
Output / color
Advanced
```

Each section should show current values, plain-language descriptions, and any compatibility/security tradeoffs.

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

### Overview

Show a concise summary:

```text
Config file
Security preset detection
Workspace policy
SSH mode
Workspace read-only status
Root filesystem read-only status
Network mode
Color behavior
Unsaved-change status
```

### Workspace

Configure:

```text
paths.workspace
workspace.risky_mount_policy
harness.pi.read_only_workspace
```

Initial editable controls toggle `workspace.risky_mount_policy` and `harness.pi.read_only_workspace`. Editing `paths.workspace` should use a later text-input flow.

### SSH

Configure:

```text
ssh.mode
ssh.selected_keys
```

The first implementation should reuse or launch the existing SSH configure flow rather than duplicating key-selection logic. Deeper integration can follow later.

### Git identity

Configure:

```text
git.inherit_host
git.user_name
git.user_email
```

The UI should show the effective identity preview based on current precedence:

```text
1. top-level git.user_name and git.user_email
2. exactly one selected SSH key with git identity metadata
3. host global Git config when git.inherit_host is true
```

### Runtime / Docker

Configure:

```text
docker.context
docker.compose_file
harness.pi.image
harness.pi.command
harness.pi.network
harness.pi.build_network
harness.pi.read_only_rootfs
```

Clearly label advanced/experimental choices, especially custom Compose files, bridge networking, and read-only rootfs.

### Output / color

Bundle remaining color policy polish here.

Recommended future config shape:

```yaml
ui:
  color: auto
```

Supported values:

```text
auto
always
never
```

Current behavior already supports non-empty `NO_COLOR` to disable colored PASS/WARN/FAIL labels. Future config-backed color behavior should preserve `NO_COLOR` as an override.

### Advanced

Expose non-everyday actions inside the TUI:

```text
config path
manual YAML editing instructions
reset section to defaults
reset all to defaults
backup/restore details
```

## Save behavior

Before writing changes:

1. Show a summary of changed fields.
2. Create a timestamped backup of `config.yaml`.
3. Save the new config.
4. Reload/validate the saved config.
5. Show the backup path and save result.

Manual YAML editing remains supported and should be mentioned in the Advanced section.

## Implementation slices

1. Add `vr config` command and read-only TUI shell.
2. Add overview/section rendering and preset detection.
3. Add save model, dirty-state prompt, and backup writer.
4. Add security preset editing with change preview.
5. Add workspace and runtime hardening editors.
6. Add `ui.color` config and output/color editor.
7. Add SSH mode editor and link to existing SSH key configure flow.
8. Add Git identity editor and effective identity preview.
9. Polish validation, reset actions, and advanced screen.
