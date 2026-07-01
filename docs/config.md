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
  compose_file: ./compose.yaml

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

## Compose file path

The default Compose file path is relative to the current working directory:

```text
./compose.yaml
```

For the MVP, run `vr` from the repo root or configure `docker.compose_file` to an appropriate path.

## State directories

The source of truth for runtime state is `~/.vegasroom`. The Compose file currently uses this path directly for bind mounts.
