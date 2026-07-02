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
};

#[derive(Debug)]
pub struct SshAddCheck {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
    pub source: String,
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

pub fn run_pi(config: &Config) -> Result<i32> {
    run_compose(config, &["run", "--rm", "pi"], true)
}

pub fn run_shell(config: &Config) -> Result<i32> {
    run_compose(config, &["run", "--rm", "pi", "sh"], true)
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
        "printf '%s\\n' \"${GIT_AUTHOR_NAME:-}\" \"${GIT_AUTHOR_EMAIL:-}\"",
    )?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let name = lines.next().unwrap_or_default().trim().to_owned();
    let email = lines.next().unwrap_or_default().trim().to_owned();

    if name.is_empty() || email.is_empty() {
        Ok(None)
    } else {
        Ok(Some(GitIdentity {
            name,
            email,
            source: "container environment".to_owned(),
        }))
    }
}

fn run_compose(config: &Config, compose_args: &[&str], warn_about_ssh: bool) -> Result<i32> {
    let mut invocation = compose_base(config, true, warn_about_ssh, SshRuntimeMode::Interactive)?;
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
    let mut invocation = compose_base(
        config,
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
    let name = config
        .git
        .user_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let email = config
        .git
        .user_email
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(GitIdentity {
        name: name.to_owned(),
        email: email.to_owned(),
        source: "~/.vegasroom/config.yaml git.user_name/git.user_email".to_owned(),
    })
}

fn single_selected_key_git_identity(keys: &[SelectedSshKey]) -> Option<GitIdentity> {
    let identities: Vec<GitIdentity> = keys
        .iter()
        .filter_map(|key| {
            let name = key
                .git_user_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            let email = key
                .git_user_email
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            Some(GitIdentity {
                name: name.to_owned(),
                email: email.to_owned(),
                source: format!("selected SSH key metadata: {}", key.path),
            })
        })
        .collect();

    if identities.len() == 1 {
        identities.into_iter().next()
    } else {
        None
    }
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

fn yaml_double_quoted_path(value: &Path) -> String {
    yaml_double_quoted_str(&value.display().to_string())
}

fn yaml_double_quoted_str(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\r', '\n'], " ")
}

fn compose_project_dir(compose_file: &Path) -> Result<PathBuf> {
    compose_file
        .parent()
        .map(Path::to_path_buf)
        .context("Compose file has no parent directory")
}

fn base_docker(config: &Config) -> Command {
    let mut command = Command::new("docker");
    command.args(["--context", config.docker.context.as_str()]);
    command
}
