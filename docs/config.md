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
