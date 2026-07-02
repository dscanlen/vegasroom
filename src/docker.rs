use std::process::{Command, Output, Stdio};

use anyhow::{anyhow, Context, Result};

use crate::{
    config::Config,
    paths::StatePaths,
    ssh::{self, SshRuntime, SshRuntimeMode},
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
    let mut invocation = compose_base(config, true, false, SshRuntimeMode::NonInteractive)?;
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

    let mut command = base_docker(config);
    command
        .arg("compose")
        .arg("-f")
        .arg(compose_file)
        .arg("--project-directory")
        .arg(project_dir);

    let ssh_runtime = if include_ssh_agent {
        let state = StatePaths::default()?;
        let runtime = ssh::prepare_agent_override(config, &state, warn_about_ssh, ssh_mode)?;
        if let Some(override_path) = runtime.override_path() {
            command.arg("-f").arg(override_path);
        }
        runtime
    } else {
        SshRuntime::empty()
    };

    Ok(ComposeInvocation {
        command,
        _ssh_runtime: ssh_runtime,
    })
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
