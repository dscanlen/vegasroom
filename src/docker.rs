use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use anyhow::{anyhow, Context, Result};

use crate::{
    config::{Config, SelectedSshKey},
    paths::{display_path, StatePaths},
    ssh::{self, SshRuntime, SshRuntimeMode},
    workspace::{self, ResolvedWorkspace},
};

#[derive(Debug)]
pub struct SshAddCheck {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn build_pi_image(config: &Config) -> Result<()> {
    let compose_file = config.resolved_compose_file()?;
    let project_dir = compose_project_dir(&compose_file)?;

    let status = base_docker(config)
        .arg("compose")
        .arg("-f")
        .arg(&compose_file)
        .arg("--project-directory")
        .arg(&project_dir)
        .args(["build", "pi"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to start Docker build command")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "Docker Compose build failed with status: {}",
            status
        ))
    }
}

pub fn run_pi(config: &Config, workspace: &ResolvedWorkspace, pi_args: &[String]) -> Result<i32> {
    let mut compose_args = vec!["run".to_owned(), "--rm".to_owned(), "pi".to_owned()];
    if !pi_args.is_empty() {
        compose_args.push(config.harness.pi.command.clone());
        compose_args.extend(pi_args.iter().cloned());
    }
    run_compose(config, workspace, &compose_args, true)
}

pub fn run_shell(config: &Config, workspace: &ResolvedWorkspace) -> Result<i32> {
    run_compose(
        config,
        workspace,
        &[
            "run".to_owned(),
            "--rm".to_owned(),
            "pi".to_owned(),
            "sh".to_owned(),
        ],
        true,
    )
}

pub fn ensure_pi_image_exists(config: &Config) -> Result<()> {
    if image_exists(config)? {
        Ok(())
    } else {
        Err(anyhow!("image not found: {}", config.harness.pi.image))
    }
}

pub fn image_exists(config: &Config) -> Result<bool> {
    let status = base_docker(config)
        .args(["image", "inspect", config.harness.pi.image.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to inspect Docker image")?;

    Ok(status.success())
}

pub fn docker_command_available() -> bool {
    Command::new("docker")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn compose_available() -> bool {
    Command::new("docker")
        .args(["compose", "version"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn context_exists(config: &Config) -> bool {
    Command::new("docker")
        .args(["context", "inspect", config.docker.context.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn context_usable(config: &Config) -> bool {
    base_docker(config)
        .arg("info")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn can_run_trivial_container(config: &Config) -> bool {
    base_docker(config)
        .args(["run", "--rm", "--network", "host", "hello-world"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn container_pi_config_writable(config: &Config) -> Result<bool> {
    compose_shell_status(
        config,
        "tmp=/home/agent/.pi/agent/.vr-m4-write-test && echo m4 > \"$tmp\" && rm -f \"$tmp\"",
    )
}

pub fn container_pi_sessions_writable(config: &Config) -> Result<bool> {
    compose_shell_status(
        config,
        "tmp=/home/agent/.pi/sessions/.vr-m4-write-test && echo m4 > \"$tmp\" && rm -f \"$tmp\"",
    )
}

pub fn container_can_reach_internet(config: &Config) -> Result<bool> {
    compose_shell_status(
        config,
        "node -e \"fetch('https://pi.dev').then(r => process.exit(r.status > 0 ? 0 : 1)).catch(() => process.exit(1))\"",
    )
}

pub fn container_receives_ssh_auth_sock(config: &Config) -> Result<Option<bool>> {
    if !ssh::planned_ssh_available(config) {
        return Ok(None);
    }

    let output = compose_shell_output(
        config,
        "test \"$SSH_AUTH_SOCK\" = '/tmp/vegasroom/ssh-agent.sock' && test -S \"$SSH_AUTH_SOCK\"",
    )?;

    Ok(Some(output.status.success()))
}

pub fn container_has_ssh_add(config: &Config) -> Result<Option<bool>> {
    if !ssh::planned_ssh_available(config) {
        return Ok(None);
    }

    let output = compose_shell_output(config, "command -v ssh-add >/dev/null")?;
    Ok(Some(output.status.success()))
}

pub fn container_ssh_add_l(config: &Config) -> Result<Option<SshAddCheck>> {
    if !ssh::planned_ssh_available(config) {
        return Ok(None);
    }

    let output = compose_shell_output(config, "ssh-add -l")?;
    Ok(Some(SshAddCheck {
        code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    }))
}

fn run_compose(
    config: &Config,
    workspace: &ResolvedWorkspace,
    compose_args: &[String],
    warn_about_ssh: bool,
) -> Result<i32> {
    let mut invocation = compose_base(
        config,
        Some(workspace),
        true,
        warn_about_ssh,
        SshRuntimeMode::Interactive,
    )?;
    let status = invocation
        .command
        .args(compose_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to start Docker Compose command")?;

    Ok(status.code().unwrap_or(1))
}

fn compose_shell_status(config: &Config, script: &str) -> Result<bool> {
    let output = compose_shell_output(config, script)?;
    Ok(output.status.success())
}

fn compose_shell_output(config: &Config, script: &str) -> Result<Output> {
    compose_shell_output_with_ssh(config, script, true)
}

fn compose_shell_output_without_ssh(config: &Config, script: &str) -> Result<Output> {
    compose_shell_output_with_ssh(config, script, false)
}

fn compose_shell_output_with_ssh(
    config: &Config,
    script: &str,
    include_ssh_agent: bool,
) -> Result<Output> {
    let workspace = workspace::default_workspace_for_compose(config)?;
    let mut invocation = compose_base(
        config,
        Some(&workspace),
        include_ssh_agent,
        false,
        SshRuntimeMode::NonInteractive,
    )?;
    invocation
        .command
        .args(["run", "--rm", "pi", "sh", "-lc", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to start Docker Compose check command")
}

struct ComposeInvocation {
    command: Command,
    _ssh_runtime: SshRuntime,
}

fn compose_base(
    config: &Config,
    workspace: Option<&ResolvedWorkspace>,
    include_ssh_agent: bool,
    warn_about_ssh: bool,
    ssh_mode: SshRuntimeMode,
) -> Result<ComposeInvocation> {
    let compose_file = config.resolved_compose_file()?;
    let project_dir = compose_project_dir(&compose_file)?;
    let state = StatePaths::default()?;

    let mut command = base_docker(config);
    command
        .arg("compose")
        .arg("-f")
        .arg(compose_file)
        .arg("--project-directory")
        .arg(project_dir);

    if let Some(workspace) = workspace {
        command.env("VR_WORKSPACE", workspace.path());
    }

    let ssh_runtime = if include_ssh_agent {
        let runtime = ssh::prepare_agent_override(config, &state, warn_about_ssh, ssh_mode)?;
        if let Some(override_path) = runtime.override_path() {
            command.arg("-f").arg(override_path);
        }
        runtime
    } else {
        SshRuntime::empty()
    };

    if let Some(git_override_path) = prepare_git_identity_override(config, &state, warn_about_ssh)?
    {
        command.arg("-f").arg(git_override_path);
    }

    Ok(ComposeInvocation {
        command,
        _ssh_runtime: ssh_runtime,
    })
}

#[derive(Debug, Clone)]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
    pub source: String,
}

pub fn effective_git_identity(config: &Config) -> Option<GitIdentity> {
    if let Some(identity) = configured_git_identity(config) {
        return Some(identity);
    }

    if let Some(identity) = single_selected_key_git_identity(&config.ssh.selected_keys) {
        return Some(identity);
    }

    if config.git.inherit_host {
        return host_git_identity();
    }

    None
}

pub fn container_git_identity(config: &Config) -> Result<Option<GitIdentity>> {
    if effective_git_identity(config).is_none() {
        return Ok(None);
    }

    let output = compose_shell_output_without_ssh(
        config,
        "printf '%s\\n%s' \"${GIT_AUTHOR_NAME:-}\" \"${GIT_AUTHOR_EMAIL:-}\"",
    )?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let name = lines
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let email = lines
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    Ok(git_identity_from_parts(
        name,
        email,
        "container environment",
    ))
}

fn prepare_git_identity_override(
    config: &Config,
    state: &StatePaths,
    warn: bool,
) -> Result<Option<PathBuf>> {
    let Some(identity) = effective_git_identity(config) else {
        if warn {
            eprintln!(
                "WARN: no Git identity configured or inherited; commits may fall back to the container user. Set git.user_name/git.user_email in ~/.vegasroom/config.yaml."
            );
        }
        return Ok(None);
    };

    fs::create_dir_all(&state.cache).with_context(|| {
        format!(
            "failed to create cache directory: {}",
            display_path(&state.cache)
        )
    })?;

    let gitconfig_path = state.cache.join("gitconfig");
    fs::write(&gitconfig_path, gitconfig_contents(&identity)).with_context(|| {
        format!(
            "failed to write Git identity config: {}",
            display_path(&gitconfig_path)
        )
    })?;

    let override_path = state.cache.join("git-identity.compose.yaml");
    let contents = format!(
        r#"services:
  pi:
    environment:
      GIT_CONFIG_GLOBAL: /tmp/vegasroom/gitconfig
      GIT_AUTHOR_NAME: "{name}"
      GIT_AUTHOR_EMAIL: "{email}"
      GIT_COMMITTER_NAME: "{name}"
      GIT_COMMITTER_EMAIL: "{email}"
    volumes:
      - type: bind
        source: "{gitconfig_path}"
        target: /tmp/vegasroom/gitconfig
        read_only: true
"#,
        name = yaml_double_quoted_str(&identity.name),
        email = yaml_double_quoted_str(&identity.email),
        gitconfig_path = yaml_double_quoted_path(&gitconfig_path),
    );

    fs::write(&override_path, contents).with_context(|| {
        format!(
            "failed to write Git identity Compose override: {}",
            display_path(&override_path)
        )
    })?;

    Ok(Some(override_path))
}

fn configured_git_identity(config: &Config) -> Option<GitIdentity> {
    let name = non_empty_trimmed(config.git.user_name.as_deref())?;
    let email = non_empty_trimmed(config.git.user_email.as_deref())?;

    Some(GitIdentity {
        name: name.to_owned(),
        email: email.to_owned(),
        source: "~/.vegasroom/config.yaml git.user_name/git.user_email".to_owned(),
    })
}

fn single_selected_key_git_identity(keys: &[SelectedSshKey]) -> Option<GitIdentity> {
    let mut identities = keys.iter().filter_map(selected_key_git_identity);
    let identity = identities.next()?;

    if identities.next().is_none() {
        Some(identity)
    } else {
        None
    }
}

fn selected_key_git_identity(key: &SelectedSshKey) -> Option<GitIdentity> {
    let name = non_empty_trimmed(key.git_user_name.as_deref())?;
    let email = non_empty_trimmed(key.git_user_email.as_deref())?;

    Some(GitIdentity {
        name: name.to_owned(),
        email: email.to_owned(),
        source: format!("selected SSH key metadata: {}", key.path),
    })
}

fn host_git_identity() -> Option<GitIdentity> {
    let name = host_git_config_value("user.name")?;
    let email = host_git_config_value("user.email")?;

    Some(GitIdentity {
        name,
        email,
        source: "host git config --global".to_owned(),
    })
}

fn host_git_config_value(key: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--global", "--get", key])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn gitconfig_contents(identity: &GitIdentity) -> String {
    format!(
        "[user]\n\tname = {}\n\temail = {}\n[safe]\n\tdirectory = *\n[init]\n\tdefaultBranch = main\n",
        git_config_value(&identity.name),
        git_config_value(&identity.email),
    )
}

fn git_config_value(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

fn git_identity_from_parts(
    name: Option<String>,
    email: Option<String>,
    source: impl Into<String>,
) -> Option<GitIdentity> {
    match (name, email) {
        (Some(name), Some(email)) => Some(GitIdentity {
            name,
            email,
            source: source.into(),
        }),
        _ => None,
    }
}

fn non_empty_trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn yaml_double_quoted_path(value: &Path) -> String {
    yaml_double_quoted_str(&value.display().to_string())
}

fn yaml_double_quoted_str(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\r', '\n'], " ")
}

fn compose_project_dir(compose_file: &std::path::Path) -> Result<std::path::PathBuf> {
    compose_file
        .parent()
        .map(std::path::Path::to_path_buf)
        .context("Compose file has no parent directory")
}

fn base_docker(config: &Config) -> Command {
    let mut command = Command::new("docker");
    command.args(["--context", config.docker.context.as_str()]);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected_key(path: &str, name: Option<&str>, email: Option<&str>) -> SelectedSshKey {
        SelectedSshKey {
            path: path.to_owned(),
            fingerprint: Some(format!("SHA256:{path}")),
            comment: None,
            key_type: Some("ED25519".to_owned()),
            git_user_name: name.map(str::to_owned),
            git_user_email: email.map(str::to_owned),
        }
    }

    #[test]
    fn git_identity_requires_name_and_email() {
        let complete = git_identity_from_parts(
            Some("Agent User".to_owned()),
            Some("agent@example.com".to_owned()),
            "test",
        );
        let missing_email = git_identity_from_parts(Some("Agent User".to_owned()), None, "test");
        let missing_name =
            git_identity_from_parts(None, Some("agent@example.com".to_owned()), "test");

        assert!(complete.is_some());
        assert!(missing_email.is_none());
        assert!(missing_name.is_none());
    }

    #[test]
    fn configured_git_identity_takes_precedence() {
        let mut config = Config::default();
        config.git.user_name = Some("Configured User".to_owned());
        config.git.user_email = Some("configured@example.com".to_owned());
        config.ssh.selected_keys.push(selected_key(
            "~/.ssh/id_ed25519",
            Some("Key User"),
            Some("key@example.com"),
        ));

        let identity = effective_git_identity(&config).unwrap();

        assert_eq!(identity.name, "Configured User");
        assert_eq!(identity.email, "configured@example.com");
        assert!(identity.source.contains("config.yaml"));
    }

    #[test]
    fn single_selected_key_git_identity_is_used() {
        let mut config = Config::default();
        config.git.inherit_host = false;
        config.ssh.selected_keys.push(selected_key(
            "~/.ssh/id_ed25519",
            Some("Key User"),
            Some("key@example.com"),
        ));

        let identity = effective_git_identity(&config).unwrap();

        assert_eq!(identity.name, "Key User");
        assert_eq!(identity.email, "key@example.com");
        assert!(identity.source.contains("selected SSH key metadata"));
    }

    #[test]
    fn multiple_selected_key_git_identities_are_ambiguous() {
        let mut config = Config::default();
        config.git.inherit_host = false;
        config.ssh.selected_keys.push(selected_key(
            "~/.ssh/id_one",
            Some("One"),
            Some("one@example.com"),
        ));
        config.ssh.selected_keys.push(selected_key(
            "~/.ssh/id_two",
            Some("Two"),
            Some("two@example.com"),
        ));

        assert!(effective_git_identity(&config).is_none());
    }

    #[test]
    fn disabled_host_inheritance_prevents_host_fallback() {
        let mut config = Config::default();
        config.git.inherit_host = false;

        assert!(effective_git_identity(&config).is_none());
    }

    #[test]
    fn gitconfig_sanitizes_newlines() {
        let identity = GitIdentity {
            name: "Agent\nUser".to_owned(),
            email: "agent\r@example.com".to_owned(),
            source: "test".to_owned(),
        };
        let contents = gitconfig_contents(&identity);

        assert!(contents.contains("name = Agent User"));
        assert!(contents.contains("email = agent @example.com"));
        assert!(!contents.contains("Agent\nUser"));
    }

    #[test]
    fn yaml_double_quoted_values_are_escaped() {
        let escaped = yaml_double_quoted_str("Agent \\\"User\"\n");

        assert_eq!(escaped, r#"Agent \\\"User\" "#);
    }
}
