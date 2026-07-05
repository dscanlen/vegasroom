mod discovery;
mod runtime;
mod status;
mod ui;

use std::{env, fs, path::PathBuf};

use crate::config::SelectedSshKey;

pub use runtime::{
    managed_keys_configured, planned_ssh_available, prepare_agent_override, selected_key_checks,
    SshRuntime, SshRuntimeMode,
};
pub use status::status;
pub use ui::configure;

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

#[derive(Debug, Clone)]
pub struct DiscoveredSshKey {
    pub path: PathBuf,
    pub display_path: String,
    pub fingerprint: Option<String>,
    pub comment: Option<String>,
    pub key_type: Option<String>,
    pub has_public_pair: bool,
    pub permissions_ok: Option<bool>,
}

impl DiscoveredSshKey {
    pub(super) fn to_selected(&self) -> SelectedSshKey {
        SelectedSshKey {
            path: self.display_path.clone(),
            fingerprint: self.fingerprint.clone(),
            comment: self.comment.clone(),
            key_type: self.key_type.clone(),
            git_user_name: None,
            git_user_email: None,
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

#[cfg(unix)]
fn is_socket(file_type: &fs::FileType) -> bool {
    use std::os::unix::fs::FileTypeExt;
    file_type.is_socket()
}

#[cfg(not(unix))]
fn is_socket(_file_type: &fs::FileType) -> bool {
    false
}
