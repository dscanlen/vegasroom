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



## Workspace path does not exist

External workspace paths must already exist. For example:

```bash
vr pi /some/external/path
```

fails if `/some/external/path` is missing. Create it first or choose an existing directory.

For managed workspace names, Vegasroom can create the directory automatically:

```bash
vr pi my-git-repo
```

This creates:

```text
~/.vegasroom/workspace/my-git-repo
```

## Workspace path is refused

Vegasroom refuses to mount credential and system paths as `/workspace`, including:

```text
/
~/.ssh
~/.config
~/.aws
~/.gcloud
~/.kube
/dev
/proc
/sys
/run
```

Choose a project directory instead, such as:

```bash
vr pi ~/workspace/my-project
vr pi .
```

## Pi option is rejected or treated as a workspace

Use the explicit separator when syntax is ambiguous:

```bash
vr pi -- --session <id>
vr pi . -- --session <id>
```

`vr pi --help` shows Vegasroom's wrapper help. To pass help through to Pi itself, run:

```bash
vr pi -- --help
```

## Managed SSH setup

If you do not want to manage `ssh-agent` manually, configure Vegasroom-managed SSH:

```bash
vr ssh configure
vr ssh status
vr doctor
```

`vr ssh configure` recursively scans `~/.ssh` by default. To scan another location:

```bash
vr ssh configure /mnt/secrethost/.ssh
```

Selected keys remain on the host. Vegasroom starts a temporary `ssh-agent`, runs `ssh-add` for selected keys, forwards only the socket into the room, and kills the agent after the room exits.

## Managed SSH key requires a passphrase

If a selected key is passphrase-protected, `vr pi` or `vr shell` may prompt through the host terminal when it runs `ssh-add`. Vegasroom does not store passphrases.

If `vr doctor` reports that managed SSH setup failed, test interactively with:

```bash
vr shell
```

Then inside the room:

```bash
ssh-add -l
ssh -T git@github.com
```

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

## Git commits use the wrong identity

Run:

```bash
vr doctor
```

Check the `Git identity` and `Room Git identity` rows. If no identity is configured, set one in:

```text
~/.vegasroom/config.yaml
```

Example:

```yaml
git:
  inherit_host: true
  user_name: Dan Scanlen
  user_email: dan@example.com
```

If `git.inherit_host` is true and `git.user_name` / `git.user_email` are empty, Vegasroom inherits the host global Git identity from:

```bash
git config --global user.name
git config --global user.email
```

Selected SSH keys may also carry `git_user_name` and `git_user_email`, but this identity is used only when exactly one selected key has a complete identity.

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
