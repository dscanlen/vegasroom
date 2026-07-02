# Managed SSH

Vegasroom can manage a temporary `ssh-agent` for a room launch so users do not need to manually start `ssh-agent`, export `SSH_AUTH_SOCK`, or run `ssh-add` before using Git over SSH.

## Commands

```bash
vr ssh configure [path...] [--follow-symlinks]
vr ssh status
```

There is intentionally no separate `detect`, `list`, `add`, `remove`, or `test` command.

- `configure` detects keys and lets the user select/deselect them.
- `status` shows the saved SSH configuration and next-launch behavior.
- `doctor` performs readiness and runtime checks.

## Detection

Default scan:

```bash
vr ssh configure
```

This recursively scans:

```text
~/.ssh
```

Explicit scan roots:

```bash
vr ssh configure /mnt/secrethost/.ssh
vr ssh configure ~/.ssh ~/work-keys
```

Symlinked directories are skipped by default. To follow them explicitly:

```bash
vr ssh configure --follow-symlinks ~/.ssh
```

Following symlinks can scan outside the requested roots and can encounter loops, so it is opt-in.

## Selector UX

The selector displays:

```text
[ ] unselected key
[✓] selected key
```

Selected rows are green. Unselected rows use the default terminal color.

The selector uses a fixed-height key list plus a details pane for the highlighted key. This keeps arrow-key navigation aligned even when paths are long. Long paths and metadata are wrapped in the details pane to the current terminal width with continuation indentation. The selector uses display-width calculations for wide glyphs such as checkboxes and arrows.

Controls:

```text
↑ / k     move highlight up
↓ / j     move highlight down
Enter     select/deselect highlighted key
Space     select/deselect highlighted key
s         save current selection and remain in the selector
q         quit
r         rescan current roots
```

If there are unsaved changes when quitting, Vegasroom prompts:

```text
Save before quitting?
  [y] save and quit
  [n] discard and quit
```

The first TUI intentionally does not implement Esc, select-all, or select-none shortcuts.

## Config

Selected keys are stored in:

```text
~/.vegasroom/config.yaml
```

Shape:

```yaml
ssh:
  mode: auto
  selected_keys:
    - path: ~/.ssh/id_ed25519
      fingerprint: SHA256:abc123...
      comment: dan@nomad
      key_type: ED25519
```

Manual editing is supported. Fingerprints are stored so Vegasroom can warn if a selected path later points to a different key.

## Modes

```text
auto
host
managed
off
```

Default:

```yaml
ssh:
  mode: auto
```

Behavior:

```text
auto:
  if managed keys are configured, start a temporary managed ssh-agent
  else if host SSH_AUTH_SOCK is usable, forward the host agent
  else warn and continue without SSH

host:
  forward only the existing host SSH_AUTH_SOCK

managed:
  always use configured selected keys through a temporary managed ssh-agent

off:
  do not forward SSH
```

## Runtime lifecycle

When managed SSH is used for `vr pi` or `vr shell`:

```text
1. Vegasroom creates a secure temp directory.
2. Vegasroom starts ssh-agent with an explicit socket path.
3. Vegasroom runs ssh-add for selected keys on the host.
4. Vegasroom writes a temporary Compose override.
5. The room receives SSH_AUTH_SOCK=/tmp/vegasroom/ssh-agent.sock.
6. Docker Compose runs the room.
7. When the room exits, Vegasroom kills the temporary ssh-agent.
8. Vegasroom removes the temp directory.
```

## Security notes

Vegasroom does not copy SSH private keys into the container.

Vegasroom does not mount host `~/.ssh` into the container.

In managed mode, Vegasroom runs `ssh-add` against selected host key files. The keys remain on the host, but the room can ask the forwarded agent socket for SSH signatures while the room is running.

Vegasroom does not store key passphrases.

Managed SSH is a usability improvement, not complete credential isolation.
