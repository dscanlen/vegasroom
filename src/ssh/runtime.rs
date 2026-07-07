use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};

use crate::{
    alert,
    config::{Config, SelectedSshKey, SshMode},
    harness,
    paths::{display_path, expand_tilde},
};

use super::{detect_host_agent, discovery::fingerprint_key, HostSshAgent, CONTAINER_SSH_AUTH_SOCK};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedKeyCheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedKeyCheck {
    pub status: SelectedKeyCheckStatus,
    pub detail: String,
}

#[derive(Debug)]
pub struct SshRuntime {
    override_path: Option<PathBuf>,
    _managed_agent: Option<ManagedSshAgent>,
}

impl SshRuntime {
    pub fn empty() -> Self {
        Self {
            override_path: None,
            _managed_agent: None,
        }
    }

    pub fn override_path(&self) -> Option<&Path> {
        self.override_path.as_deref()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SshRuntimeMode {
    Interactive,
    NonInteractive,
}

#[derive(Debug)]
struct ManagedSshAgent {
    socket: PathBuf,
    pid: String,
    temp_dir: PathBuf,
}

impl Drop for ManagedSshAgent {
    fn drop(&mut self) {
        let _ = Command::new("ssh-agent")
            .arg("-k")
            .env("SSH_AUTH_SOCK", &self.socket)
            .env("SSH_AGENT_PID", &self.pid)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        let _ = fs::remove_dir_all(&self.temp_dir);
    }
}

pub fn prepare_agent_override(
    config: &Config,
    runtime_dir: &Path,
    warn: bool,
    mode: SshRuntimeMode,
) -> Result<SshRuntime> {
    match config.ssh.mode {
        SshMode::Off => Ok(SshRuntime {
            override_path: None,
            _managed_agent: None,
        }),
        SshMode::Host => prepare_host_runtime(runtime_dir, warn),
        SshMode::Managed => prepare_managed_runtime(config, runtime_dir, mode)
            .map_err(|err| anyhow!("managed SSH agent setup failed: {err:#}")),
        SshMode::Auto => {
            if !config.ssh.selected_keys.is_empty() {
                match prepare_managed_runtime(config, runtime_dir, mode) {
                    Ok(runtime) => Ok(runtime),
                    Err(err) => {
                        if warn {
                            eprintln!("{}: managed SSH agent setup failed: {err:#}", alert::warn());
                            eprintln!(
                                "{}: falling back to host SSH_AUTH_SOCK if available",
                                alert::warn()
                            );
                        }
                        prepare_host_runtime(runtime_dir, warn)
                    }
                }
            } else {
                prepare_host_runtime(runtime_dir, warn)
            }
        }
    }
}

pub fn planned_ssh_available(config: &Config) -> bool {
    match config.ssh.mode {
        SshMode::Off => false,
        SshMode::Managed => !config.ssh.selected_keys.is_empty(),
        SshMode::Host => detect_host_agent().is_ready(),
        SshMode::Auto => !config.ssh.selected_keys.is_empty() || detect_host_agent().is_ready(),
    }
}

pub fn managed_keys_configured(config: &Config) -> bool {
    !config.ssh.selected_keys.is_empty()
}

pub fn selected_key_checks(config: &Config) -> Vec<SelectedKeyCheck> {
    let mut checks = Vec::new();

    for selected in &config.ssh.selected_keys {
        let path = expand_tilde(&selected.path);
        let display = display_path(&path);
        if !path.exists() {
            checks.push(selected_key_check(
                SelectedKeyCheckStatus::Fail,
                format!("selected SSH key missing: {display}"),
            ));
            continue;
        }

        match fingerprint_key(&path) {
            Ok(metadata) => match (&selected.fingerprint, &metadata.fingerprint) {
                (Some(expected), Some(actual)) if expected == actual => {
                    checks.push(selected_key_check(
                        SelectedKeyCheckStatus::Pass,
                        format!("selected SSH key fingerprint matches: {display}"),
                    ));
                }
                (Some(expected), Some(actual)) => {
                    checks.push(selected_key_check(
                        SelectedKeyCheckStatus::Fail,
                        format!(
                            "selected SSH key fingerprint changed: {display}; expected {expected}, got {actual}"
                        ),
                    ));
                }
                _ => checks.push(selected_key_check(
                    SelectedKeyCheckStatus::Warn,
                    format!("selected SSH key fingerprint could not be fully verified: {display}"),
                )),
            },
            Err(err) => checks.push(selected_key_check(
                SelectedKeyCheckStatus::Warn,
                format!("selected SSH key could not be inspected: {display}: {err:#}"),
            )),
        }
    }

    checks
}

fn selected_key_check(status: SelectedKeyCheckStatus, detail: String) -> SelectedKeyCheck {
    SelectedKeyCheck { status, detail }
}

fn prepare_host_runtime(runtime_dir: &Path, warn: bool) -> Result<SshRuntime> {
    let agent = detect_host_agent();
    if warn {
        if let Some(message) = agent.warning() {
            eprintln!("{message}");
        }
    }

    let override_path = match agent {
        HostSshAgent::Ready(path) => {
            Some(write_agent_compose_override_for_socket(runtime_dir, &path)?)
        }
        _ => None,
    };

    Ok(SshRuntime {
        override_path,
        _managed_agent: None,
    })
}

fn prepare_managed_runtime(
    config: &Config,
    runtime_dir: &Path,
    mode: SshRuntimeMode,
) -> Result<SshRuntime> {
    if config.ssh.selected_keys.is_empty() {
        bail!("no managed SSH keys configured. Run: vr ssh configure");
    }

    let agent = start_managed_agent()?;
    for key in &config.ssh.selected_keys {
        add_key_to_agent(&agent, key, mode)?;
    }

    let override_path = write_agent_compose_override_for_socket(runtime_dir, &agent.socket)?;
    Ok(SshRuntime {
        override_path: Some(override_path),
        _managed_agent: Some(agent),
    })
}

fn write_agent_compose_override_for_socket(
    runtime_dir: &Path,
    host_sock: &Path,
) -> Result<PathBuf> {
    fs::create_dir_all(runtime_dir).with_context(|| {
        format!(
            "failed to create per-launch runtime directory: {}",
            display_path(runtime_dir)
        )
    })?;

    let override_path = runtime_dir.join("ssh-agent.compose.yaml");
    let contents = format!(
        r#"services:
  {service_name}:
    environment:
      SSH_AUTH_SOCK: {container_sock}
    volumes:
      - type: bind
        source: "{host_sock}"
        target: {container_sock}
"#,
        service_name = harness::PI.service_name,
        container_sock = CONTAINER_SSH_AUTH_SOCK,
        host_sock = yaml_double_quoted(host_sock),
    );

    fs::write(&override_path, contents).with_context(|| {
        format!(
            "failed to write SSH agent Compose override: {}",
            display_path(&override_path)
        )
    })?;

    Ok(override_path)
}

fn start_managed_agent() -> Result<ManagedSshAgent> {
    let temp_dir = unique_agent_dir();
    fs::create_dir_all(&temp_dir).with_context(|| {
        format!(
            "failed to create temporary ssh-agent directory: {}",
            temp_dir.display()
        )
    })?;
    set_private_dir_permissions(&temp_dir)?;

    let socket = temp_dir.join("agent.sock");
    let output = Command::new("ssh-agent")
        .arg("-a")
        .arg(&socket)
        .arg("-s")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to start ssh-agent")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!("ssh-agent failed to start: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(pid) = parse_agent_pid(&stdout) else {
        bail!("ssh-agent started but SSH_AGENT_PID could not be parsed");
    };

    Ok(ManagedSshAgent {
        socket,
        pid,
        temp_dir,
    })
}

fn add_key_to_agent(
    agent: &ManagedSshAgent,
    key: &SelectedSshKey,
    mode: SshRuntimeMode,
) -> Result<()> {
    let path = expand_tilde(&key.path);
    if !path.is_file() {
        bail!(
            "selected SSH key is missing or not a file: {}",
            display_path(&path)
        );
    }

    let mut command = Command::new("ssh-add");
    command
        .arg(&path)
        .env("SSH_AUTH_SOCK", &agent.socket)
        .env("SSH_AGENT_PID", &agent.pid);

    let status = match mode {
        SshRuntimeMode::Interactive => command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status(),
        SshRuntimeMode::NonInteractive => command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    }
    .with_context(|| format!("failed to run ssh-add for {}", display_path(&path)))?;

    if status.success() {
        Ok(())
    } else {
        bail!("ssh-add failed for {}", display_path(&path));
    }
}

fn parse_agent_pid(output: &str) -> Option<String> {
    for part in output.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("SSH_AGENT_PID=") {
            let pid = value.trim();
            if !pid.is_empty() {
                return Some(pid.to_owned());
            }
        }
    }
    None
}

fn unique_agent_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    env::temp_dir().join(format!("vegasroom-agent-{}-{nanos}", std::process::id()))
}

fn yaml_double_quoted(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_agent_socket_path_is_yaml_escaped() {
        let escaped = yaml_double_quoted(Path::new(r#"/tmp/agent "quoted"/sock"#));

        assert_eq!(escaped, r#"/tmp/agent \"quoted\"/sock"#);
    }
}
