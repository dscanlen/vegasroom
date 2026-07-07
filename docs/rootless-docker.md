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

M1 found that the default bridge path was not reliable on the target rootless setup. The MVP default preserves host runtime networking through `harness.pi.network: host` and host build networking through `harness.pi.build_network: host`:

```yaml
build:
  network: ${VR_PI_BUILD_NETWORK:-host}
network_mode: ${VR_PI_NETWORK_MODE:-host}
```

This is a functional tradeoff. Hardening can revisit bridge or restricted networking later.

## Bridge-network validation experiment

Do not change the default networking model without proving the full Pi flow on the target rootless Docker setup. To test bridge runtime networking while preserving the proven build path, set:

```yaml
harness:
  pi:
    network: bridge
    build_network: host
```

Then rebuild and run the normal diagnostics. `harness.pi.network` controls runtime networking and `harness.pi.build_network` controls build networking. Keep `build_network: host` during the bridge runtime experiment because BuildKit may reject `build.network: bridge`.

```bash
vr init --build
vr doctor
vr shell .
```

Inside `vr shell .`, check outbound network, SSH agent forwarding, and Git-over-SSH:

```sh
node -e "fetch('https://pi.dev').then(r => console.log(r.status)).catch(e => { console.error(e); process.exit(1) })"
echo "$SSH_AUTH_SOCK"
ssh-add -l
ssh -T git@github.com
```

Pi auth must also be part of the bridge validation because previous bridge/rootless setups have had login/callback issues. Run:

```bash
vr pi .
```

Inside Pi:

```text
/login
```

Expected auth behavior:

```text
BROWSER=echo still prints a browser URL
that URL can be opened in the host browser
the login callback/completion succeeds
auth state is written under ~/.vegasroom/harness/pi/config/auth.json
auth remains valid after exiting and relaunching vr pi .
```

M9 validation result: bridge runtime networking with host build networking reached the provider OAuth page, but the final redirect targeted a localhost callback URL on the host such as:

```text
http://localhost:<port>/auth/callback?...
```

Under host networking, that callback reaches Pi's listener in the room. Under bridge networking, host-browser `localhost` is the host loopback, not the container loopback, so the callback cannot complete unless the callback port is published or Pi supports a different callback host/port strategy. Because Pi `/login` is baseline functionality, this keeps `harness.pi.network: host` as the default.

If any of build, doctor, outbound HTTPS, Git-over-SSH, or Pi `/login` fails under bridge runtime networking, keep `harness.pi.network: host` as the default and document the failure before trying another networking model. If the build fails with `network mode "bridge" not supported by buildkit`, set `harness.pi.build_network: host` and retest runtime bridge behavior separately.

## Common checks

```bash
docker --context rootless run --rm --network host hello-world
docker --context rootless compose version
docker --context rootless compose config
```

## Known caveats

Rootless Docker behavior depends on host kernel features, RootlessKit networking, and local Docker setup. Vegasroom does not attempt to configure rootless Docker for the user.

Use Docker's official rootless documentation for installation and host-level troubleshooting. This file only documents how Vegasroom expects to use it.
