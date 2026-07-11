# Harness-independent package and library selection

Vegasroom should eventually let users describe extra tools and libraries they want available inside a room without tying that declaration to a specific harness provider.

This document captures the design direction only. No package-selection config is active yet.

## Goals

- Let users request common project dependencies for the room environment.
- Keep dependency declarations independent of the harness provider (`pi`, future Claude/Codex harnesses, etc.).
- Avoid bloating the default image for users who do not need extra packages.
- Keep the generated environment reproducible enough to diagnose and rebuild.
- Preserve the current fast/default path for `vr init --build` and `vr pi`.

## Non-goals for the first implementation

- Full Nix/Devbox/asdf replacement.
- Per-project automatic dependency detection.
- Language-version management with every edge case covered.
- Installing arbitrary host files or secrets into images.
- Provider-specific dependency lists.

## Current base image

The current Pi harness image is based on:

```dockerfile
node:24-bookworm-slim
```

The current built-in packages are intentionally small:

```text
ca-certificates
curl
fd-find
git
openssh-client
ripgrep
```

The image also installs:

```text
@earendil-works/pi-coding-agent
```

and creates the expected room/state directories for workspace, Pi state, SSH state, cache, and scratch paths.

## Proposed config shape

Use a harness-independent top-level section, not `harness.pi.packages`:

```yaml
environment:
  apt:
    packages:
      - build-essential
      - pkg-config
      - python3
      - python3-venv
  python:
    packages:
      - pytest
      - requests
  npm:
    packages:
      - typescript
      - eslint
  rust:
    crates:
      - cargo-edit
```

Open questions before activation:

- Whether `environment` or `packages` is the clearer top-level name.
- Whether language package managers should be global installs, per-room cache installs, or project-local instructions.
- Whether versions are required in the first slice.
- How to represent OS package managers on non-Debian future images.

## Build model options

### Option A: generated Dockerfile extension

Generate a derived Dockerfile from the managed harness image:

```dockerfile
FROM vegasroom/pi:local
RUN apt-get update && apt-get install -y --no-install-recommends ...
RUN npm install -g ...
RUN python3 -m pip install ...
```

Pros:

- Simple mental model.
- Reuses Docker layer caching.
- Works with current Compose build path.

Cons:

- Ties some package config to Debian/Node image assumptions.
- Needs careful cache invalidation and diagnostics.
- Python global installs need a safe policy under Debian externally-managed Python rules.

### Option B: per-launch bootstrap script

Keep the image fixed and run a generated install/bootstrap script at container start.

Pros:

- Avoids rebuilding for every package change.
- Easier to experiment.

Cons:

- Slower launches.
- Less reproducible.
- More network dependency at runtime.
- Harder to keep read-only-rootfs compatible.

### Option C: explicit user-provided image

Document that advanced users should build their own image and set `harness.<id>.image`.

Pros:

- Already partly supported by `harness.pi.image`.
- Avoids Vegasroom owning package-manager complexity.

Cons:

- Not friendly enough for the desired package-selection feature.
- Provider/harness-independent declarations remain unsolved.

## Recommended path

Start with Option A for OS packages only:

```yaml
environment:
  apt:
    packages:
      - build-essential
      - pkg-config
```

Implementation outline:

1. Add docs and schema for `environment.apt.packages`.
2. Generate a derived Dockerfile under `~/.vegasroom/runtime/environment/Dockerfile`.
3. Build a derived image tag from `harness.pi.image`, for example `vegasroom/pi:local-env`.
4. Teach launch/build to use the derived image only when environment packages are configured.
5. Add `vr doctor` checks that show configured environment packages and whether the derived image exists.
6. Add language package managers later, one ecosystem at a time.

## Validation requirements

Before enabling any package-selection implementation, validate:

- `vr init --build` still works with no environment packages configured.
- Default image stays small and unchanged for users who do not opt in.
- Derived image builds through rootless Docker with the current build network policy.
- `vr doctor`, `vr pi`, and `vr shell` report actionable errors when package build fails.
- `harness.pi.read_only_rootfs` still launches with a prebuilt derived image.

## Deferred decisions

- Version pinning syntax.
- Per-project vs global package config.
- Python virtualenv location and persistence policy.
- npm global vs project-local install policy.
- Rust toolchain installation policy.
- Whether future harnesses share one derived base image or get harness-specific derived images from the same top-level declaration.
