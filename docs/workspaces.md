# Workspaces

Vegasroom mounts one host directory as `/workspace` inside the room.

The base Compose runtime stays stable. The `vr` wrapper resolves the requested host workspace and starts Compose with:

```text
VR_WORKSPACE=/resolved/host/path
```

The managed Compose file uses:

```yaml
volumes:
  - type: bind
    source: ${VR_WORKSPACE:-${HOME}/.vegasroom/workspace}
    target: /workspace
    read_only: ${VR_WORKSPACE_READ_ONLY:-false}
```

`harness.pi.read_only_workspace` controls `VR_WORKSPACE_READ_ONLY`. The default is `false`, so the agent can edit project files. When set to `true`, the resolved workspace is mounted read-only no matter how it was selected: default workspace, `.`, managed workspace name, relative path, tilde path, or absolute path.

## Commands

```bash
vr pi
vr pi .
vr pi my-git-repo
vr pi ~/workspace/my-git-repo
vr pi /home/dan/workspace/my-git-repo

vr shell
vr shell .
vr shell my-git-repo
```

`vr` with no subcommand is still equivalent to `vr pi`.

## Resolution rules

```text
no workspace argument:
  use configured default workspace:
  ~/.vegasroom/workspace

".":
  use the current host working directory

relative name without slash:
  resolve under configured workspace root:
  ~/.vegasroom/workspace/<name>

relative path with slash:
  resolve relative to current host working directory

absolute path:
  use the absolute path directly

~ path:
  expand against the host home directory
```

## Creation rules

Vegasroom may create missing directories only when the target is inside the managed workspace root.

This can be created automatically:

```bash
vr pi my-git-repo
```

because it resolves to:

```text
~/.vegasroom/workspace/my-git-repo
```

External paths must already exist:

```bash
vr pi /some/external/path
```

If the path is missing, Vegasroom fails clearly:

```text
FAIL: Workspace path does not exist: /some/external/path
Create it first or choose an existing directory.
```

## Read-only workspace mode

To inspect a project without allowing the room to write to `/workspace`, set:

```yaml
harness:
  pi:
    read_only_workspace: true
```

This is intentionally global for the Pi harness. It applies equally to:

```bash
vr pi
vr pi .
vr pi my-git-repo
vr pi /absolute/project/path
vr shell .
```

Pi state, sessions, SSH known_hosts, and cache mounts remain writable so login/session behavior and Git-over-SSH can continue to work. Only `/workspace` is read-only.

## Safety rules

Vegasroom refuses to mount:

```text
/
~/.ssh
~/.config
~/.aws
~/.gcloud
~/.kube
~/.azure
~/.docker
~/.gnupg
~/.password-store
~/.local/share/keyrings
```

It also refuses virtual system roots such as:

```text
/dev
/proc
/sys
/run
```

Vegasroom also refuses its own state directory outside the configured managed workspace root. With the default config, these are allowed:

```text
~/.vegasroom/workspace
~/.vegasroom/workspace/my-project
```

These are refused:

```text
~/.vegasroom
~/.vegasroom/cache
~/.vegasroom/harness
~/.vegasroom/runtime
~/.vegasroom/ssh
```

Vegasroom warns, but does not necessarily fail, for broad or risky mounts such as:

```text
host home directory
system paths under /etc, /usr, /var, /tmp, and similar roots
```

## Symlinks

Vegasroom validates the canonical workspace target, so symlinks to credential directories, virtual system roots, or protected Vegasroom state are refused. Safe symlinked project directories are allowed, but Vegasroom prints a warning such as:

```text
WARN: workspace path resolves through a symlink: ~/linked-project -> /real/project/path
```

## Pi argument pass-through

`vr pi --help` shows Vegasroom's Pi wrapper help.

To pass Pi-specific options, use either direct pass-through or the explicit separator.

Direct pass-through:

```bash
vr pi --session <id>
vr pi . --session <id>
vr pi my-git-repo --session <id>
```

Explicit separator:

```bash
vr pi -- --session <id>
vr pi . -- --session <id>
vr -- --session <id>
vr -- ask Pi a question
```

Top-level default pass-through is supported because `vr` defaults to `vr pi`, but direct top-level pass-through only applies when the first token begins with `-` and is not a Vegasroom help/version flag:

```bash
vr --session <id>
```

When the first Pi argument is positional or ambiguous, prefer the explicit `--` form.

## Help behavior

```bash
vr --help
```

shows top-level Vegasroom help.

```bash
vr pi --help
```

shows Vegasroom's Pi wrapper help, including workspace and pass-through syntax.

To ask Pi itself for help, use:

```bash
vr pi -- --help
```
