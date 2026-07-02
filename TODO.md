# Vegasroom TODO

This file tracks follow-up work separately from `POST-MVP-OPTIONS.md`. These items are not yet scoped as formal milestones.

## 1. Forward Pi agent options through `vr`

Goal: allow `vr` to accept and pass through Pi agent CLI options without failing argument parsing.

Examples to support or investigate:

```bash
vr --help
vr pi --help
vr pi --session <id>
vr pi <workspace> --session <id>
vr --session <id>
```

Notes:

- `vr` currently owns CLI parsing with `clap`, so unknown Pi flags may be rejected before Pi starts.
- Decide whether pass-through should be supported after `--`, directly after `vr pi`, or both.
- Preserve existing workspace selection behavior.
- Avoid accidentally treating Pi flags as workspace names.
- Document the final command syntax clearly.

Potential implementation direction:

- Add trailing var-arg capture for `vr pi`.
- Pass captured args to the Compose run command after the service name / command.
- Consider explicit `--` separator for ambiguous cases.

## 2. Fix Git commit identity used by automation

Goal: commits created from this environment should use the intended Git/GitHub profile identity instead of `root <root@...>`.

Current symptom:

```text
Committer: root <root@nomad.localdomain>
```

Tasks:

- Configure repo-local or environment-level Git identity.
- Prefer repo-local config if this should only affect Vegasroom.
- Use the GitHub profile name/email intended for this project.
- Consider GitHub noreply email if privacy is desired.
- Amend any local-only commits before pushing when identity is wrong.

Useful commands:

```bash
git config user.name "<GitHub display name>"
git config user.email "<GitHub email or noreply email>"
git commit --amend --reset-author
```

Open question:

- Should future agent-created commits always use the user's Git identity, or a distinct bot/co-author identity?

## 3. Support multiple SSH keys with repo-specific deploy-key matching

Goal: allow multiple keys to be available through Vegasroom-managed SSH while ensuring repo-specific deploy keys are used for the right repository.

Problem:

- SSH agents can hold multiple keys.
- GitHub deploy keys are often repository-specific.
- If the wrong key is offered first, auth can fail or select the wrong identity.
- Vegasroom currently forwards an agent socket but does not manage per-repo SSH identity selection.

Constraints:

- Do not copy private keys into the container.
- Do not mount host `~/.ssh` into the container.
- Do not store private key material or passphrases.
- Preserve managed temporary `ssh-agent` lifecycle.

Possible approaches to investigate:

1. Generate room-local SSH config under `~/.vegasroom/ssh/config` with host aliases such as:

   ```text
   Host github.com-owner-repo
     HostName github.com
     User git
     IdentitiesOnly yes
     IdentityAgent /tmp/vegasroom/ssh-agent.sock
   ```

   Then document clone URLs using the alias:

   ```bash
   git clone git@github.com-owner-repo:OWNER/REPO.git
   ```

2. Use per-repo Git config inside `/workspace`:

   ```bash
   git config core.sshCommand 'ssh -o IdentitiesOnly=yes -o IdentityAgent=/tmp/vegasroom/ssh-agent.sock ...'
   ```

3. Generate constrained helper commands such as:

   ```bash
   vr ssh repo add OWNER/REPO ~/.ssh/deploy_key_for_repo
   vr ssh repo list
   vr ssh repo remove OWNER/REPO
   ```

4. Explore whether selected-key metadata can include intended repo/host patterns.

Acceptance direction:

- A user can configure multiple deploy keys.
- Git operations for repo A use repo A's deploy key.
- Git operations for repo B use repo B's deploy key.
- Private keys remain on the host and are only loaded into the temporary managed agent.
- The room receives only the agent socket and generated non-secret SSH config/state.
