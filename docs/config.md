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
- `docker.context`
- `docker.compose_file`
- `harness.pi.image`
- `ssh.mode`
- `ssh.selected_keys`
- `git.inherit_host`
- `git.user_name`
- `git.user_email`

Currently parsed but mostly future-facing:

- `paths.root`
- `paths.workspace`
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

The source of truth for persistent state is `~/.vegasroom`. The managed Compose file currently uses this path directly for bind mounts.


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

Vegasroom runs the current MVP container as root inside rootless Docker for bind-mount compatibility, but Git commit identity is injected separately so commits do not fall back to `root <root@...>`.

Default behavior:

```yaml
git:
  inherit_host: true
  user_name:
  user_email:
```

Resolution order:

```text
1. git.user_name + git.user_email from ~/.vegasroom/config.yaml
2. exactly one selected SSH key with git_user_name + git_user_email metadata
3. host `git config --global user.name` and `user.email`, when inherit_host is true
4. no injected identity; Git may fall back to container defaults
```

Explicit global room identity:

```yaml
git:
  inherit_host: true
  user_name: Dan Scanlen
  user_email: dan@example.com
```

Per-key identity metadata is useful for deploy keys or repo-specific keys:

```yaml
ssh:
  mode: auto
  selected_keys:
    - path: ~/.ssh/id_ed25519_vegasroom
      fingerprint: SHA256:abc123...
      comment: vegasroom-deploy
      key_type: ED25519
      git_user_name: Vegasroom Deploy
      git_user_email: vegasroom-deploy@example.com
```

Per-key identity is only selected automatically when exactly one selected SSH key has both `git_user_name` and `git_user_email`. If multiple selected keys define Git identities, set the top-level `git.user_name` and `git.user_email` explicitly for the room.

At launch, Vegasroom writes generated files under `~/.vegasroom/cache/` and injects:

```text
GIT_CONFIG_GLOBAL=/tmp/vegasroom/gitconfig
GIT_AUTHOR_NAME
GIT_AUTHOR_EMAIL
GIT_COMMITTER_NAME
GIT_COMMITTER_EMAIL
```

The generated global Git config also marks container workspaces as safe to avoid Git dubious-ownership failures in the rootless container runtime.
