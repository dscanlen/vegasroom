# Security Notes

Vegasroom MVP is functional containment, not a hardened sandbox.

## What the MVP does

- Runs Pi inside an ephemeral Docker container.
- Removes the container after exit with `compose run --rm`.
- Persists only explicit bind mounts, with `/workspace` resolved by the `vr` wrapper.
- Does not mount host `~/.ssh`.
- Does not copy SSH private keys into the container.
- Forwards an SSH agent socket only when available.
- Can start a temporary managed `ssh-agent` using user-selected host keys.
- Enables `no-new-privileges:true` for the room container.
- Drops the default Linux capability set with `cap_drop: ALL`.
- Enables Docker's minimal init process for child-process reaping.
- Supports an opt-in read-only `/workspace` mount with `harness.pi.read_only_workspace: true`.
- Supports an opt-in read-only container root filesystem with `harness.pi.read_only_rootfs: true`.
- Persists in-room Pi npm-global updates only through the explicit `~/.vegasroom/harness/pi/npm-global` bind mount.

## What the MVP does not provide

- It is not complete credential isolation.
- It is not a hardened sandbox.
- It does not restrict network access.
- It does not yet run as a non-root container user by default.
- It does not manage provider API keys or secrets.

## Important tradeoffs

### Container root

The container currently runs as root. This was retained because it works with rootless Docker bind mounts on the target system.

Root inside a rootless Docker daemon is not the same as host root, but this is still a tradeoff. Current hardening keeps the proven rootless-Docker bind-mount model while adding `no-new-privileges:true` and `cap_drop: ALL` to reduce the power of container root.

A non-root runtime experiment using the image's `node` user was tested during M9 and failed the baseline workspace-write requirement: `touch /workspace/vr-node-write-test` returned `Permission denied` on the rootless Docker bind mount. Because editing the mounted workspace is core agent functionality, container root remains the default and non-root runtime is deferred unless a future UID/GID mapping approach preserves workspace, Pi state, SSH, Git, and login behavior.

### Host networking

The MVP default uses:

```yaml
build.network: host
network_mode: host
```

These values come from `harness.pi.network` and `harness.pi.build_network`, which both default to `host`. This preserves M1-M4 functionality, including rootless build behavior and login compatibility. It is not a network isolation model.

Bridge runtime networking is a hardening candidate, but it must be validated before becoming a default. Build networking is tracked separately through `harness.pi.build_network`; keep it on the proven `host` setting if BuildKit rejects `bridge`. A successful bridge-runtime test must include Docker image build, `vr doctor`, outbound HTTPS from the room, Git-over-SSH, and Pi `/login` with `BROWSER=echo` URL printing, host-browser completion, auth file persistence, and successful relaunch without repeated login.

M9 bridge validation did not pass the Pi auth requirement: OAuth could open in the host browser, but the final redirect used a `localhost:<port>` callback that is reachable with host networking and not reachable from the host browser when Pi is isolated behind bridge networking. Host networking therefore remains the proven default for login compatibility.

### Read-only root filesystem

`harness.pi.read_only_rootfs: true` enables a per-launch Compose override with:

```yaml
read_only: true
tmpfs:
  - /tmp
  - /run
  - /var/tmp
```

This reduces unintended write surface inside the live container by making image/system paths such as `/usr`, `/etc`, `/root`, and `/var` read-only unless another explicit mount covers them. It does not make host bind mounts read-only. `/workspace`, Pi state, SSH state, and cache keep their own configured mount behavior.

This option is disabled by default until Pi, login, SSH, Git, and common shell workflows are validated on target hosts.

### Read-write mounts

The selected workspace is read-write by default, and Pi state mounts remain read-write:

```text
/workspace -> resolved host workspace, read-write by default
~/.vegasroom/harness/pi
~/.vegasroom/ssh
~/.vegasroom/cache
```

Set `harness.pi.read_only_workspace: true` to mount only `/workspace` read-only. This applies to default and explicit workspace selections. Pi state, SSH known_hosts, and cache mounts remain writable so login/session behavior and Git-over-SSH can continue to work.

Processes inside the room can modify writable mounted paths. The Vegasroom SSH state directory is mounted once at `/home/agent/.ssh`; `/root/.ssh` is an image-level symlink to `/home/agent/.ssh` so root-run SSH/Git commands use the same managed state without a second host bind mount.

Workspace selection includes safety checks. Vegasroom refuses to mount `/`, virtual system roots, common credential directories such as `~/.ssh`, `~/.config`, `~/.aws`, `~/.gcloud`, and `~/.kube`, and Vegasroom state outside the configured managed workspace root. It validates canonical targets, so symlinks to blocked targets are refused. Safe symlinked project paths are allowed with a warning. By default Vegasroom still only warns before broad mounts such as the host home directory or system paths; set `workspace.risky_mount_policy: deny` to refuse those warning-level risky mounts before Docker starts. These checks reduce accidental exposure, but they are not a complete sandboxing policy.

### SSH agent forwarding

Forwarding an ssh-agent socket lets processes inside the container request SSH signatures from identities loaded in that agent.

Private key files are not copied, but the mounted socket can still authorize SSH operations.

In managed SSH mode, Vegasroom runs `ssh-add` against selected private key files on the host, forwards only the temporary agent socket, and kills the temporary agent when the room exits. Vegasroom does not store key passphrases and does not mount host `~/.ssh` into the container.

### Git identity injection

When a Git identity is configured or inherited, Vegasroom writes a generated gitconfig under a per-launch directory in `~/.vegasroom/cache` and mounts it read-only into the room. The generated file contains commit author/committer name and email only; it does not contain SSH private keys or Git credentials. Per-launch generated runtime files are removed on normal exit on a best-effort basis.

### Pi auth and package state

Pi login state may persist under:

```text
~/.vegasroom/harness/pi/config/auth.json
```

In-room global npm installs/updates for Pi may persist under:

```text
~/.vegasroom/harness/pi/npm-global
```

That prefix's `bin` directory is before the baked image install on `PATH`, so executable code persisted there is trusted by future rooms. Treat the Pi harness state directory as sensitive.

## Deferred hardening

Post-MVP work should revisit:

- non-root container user through a UID/GID mapping approach that preserves bind-mount writes
- network restrictions
- stricter mount policy and optional confirmation prompts
- proving whether `harness.pi.read_only_rootfs` can become a safe default
- making stricter workspace mount policies easier to use interactively
- clearer credential lifecycle controls
