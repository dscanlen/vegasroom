# Managed SSH

Vegasroom can manage a temporary `ssh-agent` for a room launch so users do not need to manually start `ssh-agent`, export `SSH_AUTH_SOCK`, or run `ssh-add` before using Git over SSH.

## Commands

```bash
vr config
vr doctor
```

There is intentionally no separate `detect`, `list`, `add`, `remove`, or `test` command.

- `vr config` opens the SSH key selector from the SSH menu item.
- `vr doctor` shows saved SSH configuration and next-launch behavior as part of readiness checks.

## Detection

Default scan from `vr config` recursively scans:

```text
~/.ssh
```

The current public TUI entry point uses the default scan root. Manual YAML editing remains supported for selected keys when needed.

Symlinked directories are skipped by default. Following symlinks can scan outside requested roots and can encounter loops, so it remains an internal opt-in path rather than a public config command.

## Selector UX

The selector displays:

```text
○ unselected key
✓ selected key
```

Selected rows are green. Unselected rows use the default terminal color.

The selector uses a bottom-aligned panel, a fixed-height key list, and compact metadata for the highlighted key. Long paths and metadata are truncated to the terminal width. The selector uses display-width calculations for wide glyphs and renders rows by absolute terminal coordinates so raw-mode terminals do not produce stepped line layouts.

Controls:

```text
↑ / k     move highlight up
↓ / j     move highlight down
Enter     select/deselect highlighted key
s         save current selection and remain in the selector
Esc/q     quit
r         rescan current roots
```

If there are unsaved changes when quitting, Vegasroom prompts:

```text
Save changes before quitting?
  y  save and quit
  n  discard and quit
  c  cancel
```

The first TUI intentionally does not implement select-all or select-none shortcuts.

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
      git_user_name: Dan Scanlen
      git_user_email: dan@example.com
```

`git_user_name` and `git_user_email` are optional. They let a selected key carry a repo/deploy-key-specific commit identity. Manual editing is supported. Fingerprints are stored so Vegasroom can warn if a selected path later points to a different key.

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
5. The room receives SSH_AUTH_SOCK=/run/vegasroom-ssh-agent.sock.
6. Docker Compose runs the room.
7. When the room exits, Vegasroom kills the temporary ssh-agent.
8. Vegasroom removes the temp directory.
```

## Security notes

Vegasroom does not copy SSH private keys into the container.

Vegasroom does not mount host `~/.ssh` into the container. The managed SSH state directory is mounted once at `/home/agent/.ssh`; `/root/.ssh` is an image-level symlink to `/home/agent/.ssh` for root-run SSH/Git compatibility.

In managed mode, Vegasroom runs `ssh-add` against selected host key files. The keys remain on the host, but the room can ask the forwarded agent socket for SSH signatures while the room is running.

Vegasroom does not store key passphrases.

Managed SSH is a usability improvement, not complete credential isolation.


## Git commit identity

SSH authentication and Git commit authorship are separate. A successful SSH key proves transport access to GitHub, but it does not automatically set `git config user.name` or `user.email` inside the room. Deploy keys especially do not expose a human author profile.

Vegasroom injects Git identity at runtime using this order:

```text
1. top-level git.user_name/git.user_email
2. exactly one selected SSH key with git_user_name/git_user_email
3. host global Git config when git.inherit_host is true
```

This prevents commits from falling back to `root <root@...>` while preserving the rootless-container runtime model. The resolved identity is injected through a per-launch generated Compose override, commit-author/committer environment variables, and a read-only generated gitconfig mounted inside the room.
