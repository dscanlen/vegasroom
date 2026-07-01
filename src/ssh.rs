use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::paths::{display_path, StatePaths};

pub const CONTAINER_SSH_AUTH_SOCK: &str = "/tmp/vegasroom/ssh-agent.sock";

#[derive(Debug, Clone)]
pub enum HostSshAgent {
    Ready(PathBuf),
    MissingEnv,
    MissingPath(PathBuf),
    NotSocket(PathBuf),
}

impl HostSshAgent {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    pub fn status_detail(&self) -> String {
        match self {
            Self::Ready(path) => format!("SSH_AUTH_SOCK is a socket: {}", path.display()),
            Self::MissingEnv => "SSH_AUTH_SOCK is not set. Git over SSH may not work inside the room.".to_owned(),
            Self::MissingPath(path) => format!(
                "SSH_AUTH_SOCK points to a missing path: {}. Git over SSH may not work inside the room.",
                path.display()
            ),
            Self::NotSocket(path) => format!(
                "SSH_AUTH_SOCK does not appear to be a socket: {}. Git over SSH may not work inside the room.",
                path.display()
            ),
        }
    }

    pub fn warning(&self) -> Option<String> {
        if self.is_ready() {
            None
        } else {
            Some(format!("WARN: {}", self.status_detail()))
        }
    }
}

pub fn detect_host_agent() -> HostSshAgent {
    let Ok(raw) = env::var("SSH_AUTH_SOCK") else {
        return HostSshAgent::MissingEnv;
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return HostSshAgent::MissingEnv;
    }

    let path = PathBuf::from(trimmed);
    let Ok(metadata) = fs::metadata(&path) else {
        return HostSshAgent::MissingPath(path);
    };

    if is_socket(&metadata.file_type()) {
        HostSshAgent::Ready(path)
    } else {
        HostSshAgent::NotSocket(path)
    }
}

pub fn write_agent_compose_override(
    state: &StatePaths,
    agent: &HostSshAgent,
) -> Result<Option<PathBuf>> {
    let HostSshAgent::Ready(host_sock) = agent else {
        return Ok(None);
    };

    fs::create_dir_all(&state.cache).with_context(|| {
        format!(
            "failed to create cache directory: {}",
            display_path(&state.cache)
        )
    })?;

    let override_path = state.cache.join("ssh-agent.compose.yaml");
    let contents = format!(
        r#"services:
  pi:
    environment:
      SSH_AUTH_SOCK: {container_sock}
    volumes:
      - type: bind
        source: "{host_sock}"
        target: {container_sock}
"#,
        container_sock = CONTAINER_SSH_AUTH_SOCK,
        host_sock = yaml_double_quoted(host_sock),
    );

    fs::write(&override_path, contents).with_context(|| {
        format!(
            "failed to write SSH agent Compose override: {}",
            display_path(&override_path)
        )
    })?;

    Ok(Some(override_path))
}

pub fn prepare_agent_override(state: &StatePaths, warn: bool) -> Result<Option<PathBuf>> {
    let agent = detect_host_agent();
    if warn {
        if let Some(message) = agent.warning() {
            eprintln!("{message}");
        }
    }
    write_agent_compose_override(state, &agent)
}

fn yaml_double_quoted(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(unix)]
fn is_socket(file_type: &fs::FileType) -> bool {
    use std::os::unix::fs::FileTypeExt;
    file_type.is_socket()
}

#[cfg(not(unix))]
fn is_socket(_file_type: &fs::FileType) -> bool {
    false
}
