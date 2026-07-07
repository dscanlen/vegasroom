# Configuration

Vegasroom config lives at:

```text
~/.vegasroom/config.yaml
```

`vr init` creates this file if it is missing. Existing config is not overwritten silently.

## Default config

```yaml
paths:
  workspace: ~/.vegasroom/workspace

workspace:
  risky_mount_policy: warn

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
    image: vegasroom/pi:local
    command: pi
    network: host
    build_network: host
    read_only_workspace: false
    read_only_rootfs: false
```

## Active fields

Currently active:

- `paths.workspace`
- `workspace.risky_mount_policy`
- `docker.context`
- `docker.compose_file`
- `harness.pi.image`
- `harness.pi.command`
- `harness.pi.network`
- `harness.pi.build_network`
- `harness.pi.read_only_workspace`
- `harness.pi.read_only_rootfs`
- `ssh.mode`
- `ssh.selected_keys`
- `git.inherit_host`
- `git.user_name`
- `git.user_email`

Legacy/future-facing fields from earlier configs are ignored if present:

- `default_harness`
- `paths.root`
- `harness.pi.enabled`
- `harness.pi.ssh_agent`
- Claude harness config

## Future multi-harness config direction

The active config still only supports the Pi harness. Before adding Claude Code or Codex, the intended near-term shape is to keep each harness under `harness.<id>`:

```yaml
harness:
  pi:
    image: vegasroom/pi:local
    command: pi
    network: host
    build_network: host
    read_only_workspace: false
    read_only_rootfs: false
  claude:
    image: vegasroom/claude:local
    command: claude
  codex:
    image: vegasroom/codex:local
    command: codex
```

This is documentation of direction, not active config. For now, Docker/runtime hardening fields remain on `harness.pi` until a second harness proves which settings should become shared. Avoid introducing a top-level `runtime:` config section until that need is validated.

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

`docker.compose_file` controls the Compose file passed to Docker. The default is the Vegasroom-managed runtime file.

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

## Workspace policy fields

`workspace.risky_mount_policy` controls what Vegasroom does with broad or risky workspace mounts that are not already hard-blocked. Supported values are:

```text
warn  print a warning and continue, preserving the current default behavior
deny  refuse the risky workspace before Docker starts
```

Credential directories, virtual system roots, `/`, and Vegasroom state outside the configured workspace root are always refused regardless of this policy. The policy applies to warning-level paths such as the host home directory and risky system paths under `/tmp`, `/etc`, `/usr`, `/var`, and similar roots.


## Pi harness runtime fields

`harness.pi.image` controls the Compose image name used for build, image inspection, and runtime launch. Vegasroom passes it to Compose through `VR_PI_IMAGE`.

`harness.pi.command` controls the command executed by `vr pi`, both with and without Pi arguments. The default is:

```yaml
harness:
  pi:
    command: pi
```

For example, this runs `pi --session <id>` inside the room:

```bash
vr pi --session <id>
```

`harness.pi.network` controls the configured Docker network mode for the room runtime. Vegasroom passes it to Compose through `VR_PI_NETWORK_MODE`. The default remains `host` because that is the proven rootless Docker model. Treat non-host values such as `bridge` as validation experiments until outbound HTTPS, Git-over-SSH, and Pi `/login` have all been proven on the target rootless Docker setup. M9 bridge validation did not pass Pi `/login` because the OAuth flow redirected the host browser to a container-local `localhost:<port>` callback.

`harness.pi.build_network` controls the Docker build network mode. Vegasroom passes it to Compose through `VR_PI_BUILD_NETWORK`. The default remains `host` because that is the proven rootless Docker build model. Keep this as `host` while testing `harness.pi.network: bridge`; BuildKit may reject `build.network: bridge`.

`harness.pi.read_only_workspace` controls whether the resolved host workspace is mounted read-only at `/workspace`. The default is `false` so Pi can edit project files. When set to `true`, it applies to the default workspace and to explicit workspace arguments such as `vr pi .`, `vr pi my-repo`, and `vr pi /path/to/repo`.

`harness.pi.read_only_rootfs` controls an opt-in read-only container root filesystem experiment. The default is `false`. When set to `true`, Vegasroom adds a per-launch Compose override with `read_only: true` and writable tmpfs scratch paths for `/tmp`, `/run`, and `/var/tmp`. Explicit bind mounts such as `/workspace`, Pi state, SSH state, and cache keep their configured write behavior.

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
