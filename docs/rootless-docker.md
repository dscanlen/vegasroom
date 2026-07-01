# Rootless Docker Notes

Vegasroom MVP targets Linux with Docker running through a rootless context named:

```text
rootless
```

## Check contexts

```bash
docker context ls
docker --context rootless info
```

`vr doctor` checks that the configured context exists and responds to Docker commands.

## Why `--context rootless`

Vegasroom launches agent workloads through the host Docker CLI while avoiding a rootful Docker daemon for the MVP target.

The command shape remains:

```bash
docker --context rootless compose run --rm pi
```

## Compose networking

M1 found that the default bridge path was not reliable on the target rootless setup. The MVP preserves:

```yaml
build:
  network: host
network_mode: host
```

This is a functional tradeoff. Hardening can revisit bridge or restricted networking later.

## Common checks

```bash
docker --context rootless run --rm --network host hello-world
docker --context rootless compose version
docker --context rootless compose config
```

## Known caveats

Rootless Docker behavior depends on host kernel features, RootlessKit networking, and local Docker setup. Vegasroom does not attempt to configure rootless Docker for the user.

Use Docker's official rootless documentation for installation and host-level troubleshooting. This file only documents how Vegasroom expects to use it.
