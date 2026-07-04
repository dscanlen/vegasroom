# Configuration

Vegasroom config lives at:

```text
~/.vegasroom/config.yaml
```

`vr init` creates this file if it is missing. Existing config is not overwritten silently.

## Default config

```yaml
default_harness: pi

paths:
  root: ~/.vegasroom
  workspace: ~/.vegasroom/workspace

docker:
  context: rootless
  compose_file: ~/.vegasroom/runtime/compose.yaml

ssh:
  mode: auto
  selected_keys: []

git:
  inherit_host: true
  user_name:
  user_email:

harness:
  pi:
    enabled: true
    image: vegasroom/pi:local
    command: pi
    ssh_agent: auto
    network: host

  # claude:
  #   enabled: false
  #   image: vegasroom/claude:local
  #   command: claude
  #   ssh_agent: auto
  #   network: host
```

## Active fields

Currently active:

- `default_harness`
- `paths.workspace`
- `docker.context`
- `docker.compose_file`
- `harness.pi.image`
- `harness.pi.command`
- `ssh.mode`
- `ssh.selected_keys`
- `git.inherit_host`
- `git.user_name`
- `git.user_email`

Currently parsed but mostly future-facing:

- `paths.root`
- `harness.pi.enabled`
- `harness.pi.command`
- `harness.pi.ssh_agent`
- `harness.pi.network`
- commented Claude config

## Managed runtime path

The generated template points to the managed Compose file:

```text
~/.vegasroom/runtime/compose.yaml
```

The installed `vr` binary embeds the MVP Compose file and Pi Dockerfile at compile time. `vr init` writes those files into:

```text
~/.vegasroom/runtime/compose.yaml
~/.vegasroom/runtime/harness/pi/Dockerfile
```

Docker Compose is then invoked with `--project-directory ~/.vegasroom/runtime`, so installed `vr` commands work from any current directory and do not require the original git checkout to remain on disk.

`docker.compose_file` is still stored in config for visibility and future flexibility, but the MVP default is the Vegasroom-managed runtime file.

## State directories

The source of truth for persistent state is `~/.vegasroom`.

`paths.workspace` controls the default workspace root. The default is:

```text
~/.vegasroom/workspace
```

`vr pi` and `vr shell` use that path when no workspace argument is provided. A relative workspace name resolves below this root:

```bash
vr pi my-git-repo
```

resolves to:

```text
~/.vegasroom/workspace/my-git-repo
```

The managed Compose file receives the resolved host workspace through `VR_WORKSPACE` and mounts it at `/workspace`.


## Pi command

`harness.pi.command` is used when `vr pi` passes arguments through to Pi. The default is:

```yaml
harness:
  pi:
    command: pi
```

For example, this runs `pi --session <id>` inside the room:

```bash
vr pi --session <id>
```

## SSH config

Managed SSH stores selected key references in config. Private key contents and passphrases are not stored.

Example:

```yaml
ssh:
  mode: auto
  selected_keys:
    - path: ~/.ssh/id_ed25519
      fingerprint: SHA256:abc123...
      comment: dan@nomad
      key_type: ED25519
```

Supported modes:

```text
auto     use managed keys if configured, otherwise host SSH_AUTH_SOCK if available
host     only forward the existing host SSH_AUTH_SOCK
managed  always start a temporary Vegasroom-managed ssh-agent
off      do not forward SSH
```

Use `vr ssh configure` to edit this interactively, or edit the YAML manually.

## Git identity

SSH authentication and Git commit identity are separate. Vegasroom injects Git identity into the room with per-launch generated runtime files so commits do not fall back to the container user when an identity is configured.

Precedence:

```text
1. top-level git.user_name and git.user_email
2. exactly one selected SSH key with git_user_name and git_user_email
3. host global Git config when git.inherit_host is true
```

Example top-level identity:

```yaml
git:
  inherit_host: true
  user_name: Dan Scanlen
  user_email: dan@example.com
```

Example selected-key identity metadata:

```yaml
ssh:
  selected_keys:
    - path: ~/.ssh/id_ed25519
      fingerprint: SHA256:abc123...
      git_user_name: Dan Scanlen
      git_user_email: dan@example.com
```
