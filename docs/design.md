# Vegasroom Design Notes

Vegasroom is a small host CLI that launches Pi inside an ephemeral Docker Compose service.

## MVP goal

Provide a usable source-built tool that lets a user run Pi in a repeatable container environment without manually invoking Docker Compose.

## Runtime flow

```text
vr
  -> Rust CLI
  -> docker --context rootless compose run --rm pi
  -> ephemeral Pi container
```

Persistent host state is limited to explicit bind mounts under `~/.vegasroom`.

## Commands

```bash
vr init
vr doctor
vr config
vr pi
vr shell
vr
```

`vr` defaults to `vr pi`.

## State model

```text
~/.vegasroom/
  config.yaml
  workspace/
  harness/pi/config/
  harness/pi/extensions/
  harness/pi/skills/
  harness/pi/sessions/
  harness/pi/npm-global/
  environment/cargo/
  ssh/
  cache/
```

## Container model

MVP-preserved runtime decisions:

- rootless Docker context named `rootless`
- Docker Compose service `pi`
- default image `vegasroom/pi:latest` from `harness.pi.image`, with standard builds also tagged as `vegasroom/pi:<vr-version>`
- container-root runtime for now
- `no-new-privileges:true`, `cap_drop: ALL`, and `init: true` runtime hardening
- default `build.network=host` from `harness.pi.build_network`
- default `network_mode=host` from `harness.pi.network`
- workspace mounted read-write by default, with opt-in `harness.pi.read_only_workspace`
- container root filesystem writable by default, with opt-in `harness.pi.read_only_rootfs`
- Pi state mounted read-write
- Pi npm-global prefix mounted read-write at `/home/agent/.npm-global`, with `/home/agent/.npm-global/bin` before `/usr/local/bin` on `PATH`, so in-room Pi npm updates persist across ephemeral containers while the pinned image-baked install remains a fallback
- optional environment config, starting with `environment.apt.packages`, `environment.rust`, `environment.python`, `environment.go`, and `environment.typescript`, generates a derived image such as `vegasroom/pi:latest-env`, which is rebuilt when the requested package/toolchain set changes
- Cargo cache/install state persists through `~/.vegasroom/environment/cargo` mounted at `/home/agent/.cargo`
- Vegas-managed SSH directory mounted once at `/home/agent/.ssh`, not host `~/.ssh`
- `/root/.ssh` provided as an image-level symlink to `/home/agent/.ssh` for root-run SSH/Git compatibility without a second bind mount
- ssh-agent socket forwarded when available

## Why container root remains for MVP

M1 attempted a non-root container user and hit rootless Docker bind-mount permission friction. The MVP keeps container root because the Docker daemon itself runs rootless and this model has been proven to work.

This is a tradeoff, not a hardening endpoint.

## SSH model

Host private keys are not copied into the room. Host `~/.ssh` is not mounted.

When `$SSH_AUTH_SOCK` points to a real socket, the CLI generates a Compose override under a per-launch directory in `~/.vegasroom/cache` and mounts the socket into the container at:

```text
/run/vegasroom-ssh-agent.sock
```

## Login model

Pi login is handled by Pi itself. Vegasroom sets `BROWSER=echo` so browser login URLs can be opened on the host. Auth state is expected to persist through the Pi config mount:

```text
~/.vegasroom/harness/pi/config/auth.json
```

## Deferred work

- Claude harness support
- harness plugin abstraction
- harness-independent package/library selection; see [Package selection design](package-selection.md)
- non-root container migration
- network isolation
- hardening profiles
- provider/API-key management
- installer and package distribution
