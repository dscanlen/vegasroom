# Troubleshooting

## Docker was not found

Install Docker and ensure `docker` is on `PATH`:

```bash
docker --version
```

## Docker Compose is missing

Vegasroom expects Compose v2 through:

```bash
docker compose version
```

## Rootless context missing

Check contexts:

```bash
docker context ls
```

Vegasroom defaults to:

```text
rootless
```

Create or select a rootless Docker context before running the room, or update `~/.vegasroom/config.yaml`.

## Pi image missing

Build the local image:

```bash
vr init --build
```

Source workflow:

```bash
cargo run -- init --build
```

## Build fails with rootless networking errors

Confirm the Compose file preserves:

```yaml
build:
  network: host
network_mode: host
```

Run:

```bash
docker --context rootless compose config
```

## State path exists as the wrong type

If a required directory exists as a file, `vr init` fails intentionally.

Example:

```text
Expected directory path exists as a file: ~/.vegasroom/ssh
```

Move or delete the conflicting file, then rerun:

```bash
vr init
```


## `vr` only works from the repo directory

Current Vegasroom should not require the repo directory after installation. The installed binary embeds the runtime files and `vr init` materializes them into:

```text
~/.vegasroom/runtime/compose.yaml
~/.vegasroom/runtime/harness/pi/Dockerfile
```

Repair the managed runtime files and config with:

```bash
vr init
```

Then check:

```bash
grep -A3 '^docker:' ~/.vegasroom/config.yaml
ls -la ~/.vegasroom/runtime ~/.vegasroom/runtime/harness/pi
```

The config should point to:

```yaml
docker:
  compose_file: ~/.vegasroom/runtime/compose.yaml
```

After repair, `vr doctor`, `vr pi`, and `vr shell` should work from any current directory, even if the original git checkout has been removed.

## SSH_AUTH_SOCK missing

Start an agent and add a key:

```bash
eval "$(ssh-agent -s)"
ssh-add ~/.ssh/id_ed25519
ssh-add -l
```

Then run:

```bash
vr doctor
vr shell
```

## ssh-agent has no identities

`ssh-add -l` returns no identities. Add the key you use for Git:

```bash
ssh-add ~/.ssh/id_ed25519
```

## GitHub host key prompt

The room uses Vegas-managed SSH state:

```text
~/.vegasroom/ssh/known_hosts
```

Accepting GitHub's host key inside the room records it there, not in host `~/.ssh`.

## Git clone over SSH fails

Check on the host:

```bash
echo "$SSH_AUTH_SOCK"
ssh-add -l
ssh -T git@github.com
```

Then check inside the room:

```bash
vr shell
echo "$SSH_AUTH_SOCK"
ssh-add -l
ssh -T git@github.com
```

Private keys should not appear inside `/root/.ssh` or `/home/agent/.ssh`.

## Pi login does not persist

After `/login`, inspect:

```bash
ls -la ~/.vegasroom/harness/pi/config
```

The expected auth file is:

```text
~/.vegasroom/harness/pi/config/auth.json
```

Then relaunch:

```bash
vr pi
```

## Network issues

The MVP uses host networking. Check from the room:

```bash
vr shell
node -e "fetch('https://pi.dev').then(r => console.log(r.status)).catch(e => { console.error(e); process.exit(1) })"
```

## Permission problems with bind mounts

Run:

```bash
vr init
vr doctor
```

The MVP intentionally uses container root inside rootless Docker because earlier non-root attempts hit bind-mount write issues.
