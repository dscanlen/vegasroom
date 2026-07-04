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

## What the MVP does not provide

- It is not complete credential isolation.
- It is not a hardened sandbox.
- It does not restrict network access.
- It does not run with a minimized capability profile.
- It does not manage provider API keys or secrets.

## Important tradeoffs

### Container root

The container currently runs as root. This was retained because it works with rootless Docker bind mounts on the target system.

Root inside a rootless Docker daemon is not the same as host root, but this is still a tradeoff.

### Host networking

The MVP uses:

```yaml
build.network: host
network_mode: host
```

This preserves M1-M4 functionality, including rootless build behavior and login compatibility. It is not a network isolation model.

### Read-write mounts

The selected workspace and Pi state mounts are read-write:

```text
/workspace -> resolved host workspace
~/.vegasroom/harness/pi
~/.vegasroom/ssh
~/.vegasroom/cache
```

Processes inside the room can modify these paths.

Workspace selection includes safety checks. Vegasroom refuses to mount `/`, virtual system roots, and common credential directories such as `~/.ssh`, `~/.config`, `~/.aws`, `~/.gcloud`, and `~/.kube`. It warns before broad mounts such as the host home directory or system paths. These checks reduce accidental exposure, but they are not a complete sandboxing policy.

### SSH agent forwarding

Forwarding an ssh-agent socket lets processes inside the container request SSH signatures from identities loaded in that agent.

Private key files are not copied, but the mounted socket can still authorize SSH operations.

In managed SSH mode, Vegasroom runs `ssh-add` against selected private key files on the host, forwards only the temporary agent socket, and kills the temporary agent when the room exits. Vegasroom does not store key passphrases and does not mount host `~/.ssh` into the container.

### Git identity injection

When a Git identity is configured or inherited, Vegasroom writes a generated gitconfig under `~/.vegasroom/cache` and mounts it read-only into the room. The generated file contains commit author/committer name and email only; it does not contain SSH private keys or Git credentials.

### Pi auth state

Pi login state may persist under:

```text
~/.vegasroom/harness/pi/config/auth.json
```

Treat the Pi harness state directory as sensitive.

## Deferred hardening

Post-MVP work should revisit:

- non-root container user
- network restrictions
- capability reduction
- stricter mount policy and optional confirmation prompts
- optional read-only workspace mode
- warnings for dangerous mount paths
- clearer credential lifecycle controls
