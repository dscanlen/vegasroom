# Vegasroom

Vegasroom is an experimental CLI for launching AI agent harnesses inside ephemeral Docker containers.

M2 provides a minimal Rust command named `vr` that wraps the proven M1 Pi runtime.

## Current commands

```bash
cargo run -- init
cargo run -- doctor
cargo run -- pi
cargo run -- shell
```

When installed as `vr`:

```bash
vr init
vr doctor
vr pi
vr shell
vr
```

`vr` defaults to `vr pi`.

## Runtime assumptions

M2 preserves the M1 runtime decisions:

- Linux
- Docker with a configured `rootless` context
- Docker Compose
- host-network fallback for rootless Docker
- container-root runtime inside rootless Docker
- explicit bind mounts only
- Pi state under `~/.vegasroom/harness/pi`
- workspace under `~/.vegasroom/workspace`
- SSH directory mount at `~/.vegasroom/ssh`

## Build image

After `vr init`, build the local Pi image:

```bash
cargo run -- init --build
```

or directly:

```bash
docker --context rootless compose build pi
```

## State directory

Vegasroom creates or repairs:

```text
~/.vegasroom/
  config.yaml
  workspace/
  harness/pi/config/
  harness/pi/extensions/
  harness/pi/skills/
  harness/pi/sessions/
  ssh/
  cache/
```

Provider/API-key handling is intentionally out of scope for M2.
