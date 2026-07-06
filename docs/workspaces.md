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
```

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

Vegasroom warns, but does not necessarily fail, for broad or risky mounts such as:

```text
host home directory
system paths under /etc, /usr, /var, /tmp, and similar roots
Vegasroom state outside ~/.vegasroom/workspace
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
