use std::{env, fs, path::Path};

use super::{check_bool, command_available, Check, Status};
use crate::{config::Config, docker, ssh};

pub(super) fn check_ssh_configuration(
    config: &Config,
    host_agent: &ssh::HostSshAgent,
) -> Vec<Check> {
    let mut checks = Vec::new();

    checks.push(Check {
        status: Status::Pass,
        name: "SSH mode",
        detail: format!("configured mode is {:?}", config.ssh.mode),
    });

    if config.ssh.selected_keys.is_empty() {
        checks.push(Check {
            status: Status::Warn,
            name: "Managed SSH keys",
            detail: "no managed SSH keys configured. Run: vr ssh configure".to_owned(),
        });
    } else {
        checks.push(Check {
            status: Status::Pass,
            name: "Managed SSH keys",
            detail: format!("{} key(s) configured", config.ssh.selected_keys.len()),
        });

        for key_check in ssh::selected_key_checks(config) {
            checks.push(Check {
                status: selected_key_check_status(key_check.status),
                name: "Selected SSH key",
                detail: key_check.detail,
            });
        }
    }

    let next_launch = if ssh::managed_keys_configured(config) {
        "managed temporary ssh-agent will be used before any ambient host agent"
    } else if host_agent.is_ready() {
        "host SSH_AUTH_SOCK will be forwarded"
    } else {
        "no SSH agent will be forwarded"
    };
    checks.push(Check {
        status: if ssh::planned_ssh_available(config) {
            Status::Pass
        } else {
            Status::Warn
        },
        name: "SSH next launch",
        detail: next_launch.to_owned(),
    });

    checks.push(check_bool(
        Status::Warn,
        "Host ssh-agent binary",
        command_available("ssh-agent"),
        "ssh-agent is available on PATH",
        "ssh-agent was not found on PATH; managed SSH cannot start",
    ));

    checks.push(check_bool(
        Status::Warn,
        "Host ssh-add binary",
        command_available("ssh-add"),
        "ssh-add is available on PATH",
        "ssh-add was not found on PATH; managed SSH cannot add selected keys",
    ));

    checks
}

fn selected_key_check_status(status: ssh::SelectedKeyCheckStatus) -> Status {
    match status {
        ssh::SelectedKeyCheckStatus::Pass => Status::Pass,
        ssh::SelectedKeyCheckStatus::Warn => Status::Warn,
        ssh::SelectedKeyCheckStatus::Fail => Status::Fail,
    }
}

pub(super) fn check_config_git_section(path: &Path) -> Check {
    match fs::read_to_string(path) {
        Ok(contents) if contents.lines().any(|line| line.trim_end() == "git:") => Check {
            status: Status::Pass,
            name: "Config Git section",
            detail: "~/.vegasroom/config.yaml contains a git section".to_owned(),
        },
        Ok(_) => Check {
            status: Status::Warn,
            name: "Config Git section",
            detail: "missing from ~/.vegasroom/config.yaml; run `vr init` to add git.inherit_host/user_name/user_email defaults".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Config Git section",
            detail: format!("could not read config for git section check: {err:#}"),
        },
    }
}

pub(super) fn check_workspace_risky_mount_policy(config: &Config) -> Check {
    Check {
        status: Status::Pass,
        name: "Workspace risky mount policy",
        detail: format!(
            "workspace.risky_mount_policy is {:?}",
            config.workspace.risky_mount_policy
        ),
    }
}

pub(super) fn check_network_mode(config: &Config) -> Check {
    let detail = if config.harness.pi.network == "host" {
        format!(
            "runtime network is host and build network is {}; host runtime networking is the proven MVP default but not a network isolation model",
            config.harness.pi.build_network
        )
    } else {
        format!(
            "runtime network is {} and build network is {}; validate Docker build, internet, Git-over-SSH, and Pi /login before relying on this mode; bridge may break localhost OAuth callbacks",
            config.harness.pi.network, config.harness.pi.build_network
        )
    };

    Check {
        status: Status::Pass,
        name: "Network mode",
        detail,
    }
}

pub(super) fn check_read_only_rootfs_mode(config: &Config) -> Check {
    if config.harness.pi.read_only_rootfs {
        Check {
            status: Status::Pass,
            name: "Read-only rootfs mode",
            detail:
                "harness.pi.read_only_rootfs is true; the container root filesystem will be mounted read-only with tmpfs scratch paths"
                    .to_owned(),
        }
    } else {
        Check {
            status: Status::Pass,
            name: "Read-only rootfs mode",
            detail:
                "harness.pi.read_only_rootfs is false; the container root filesystem remains writable"
                    .to_owned(),
        }
    }
}

pub(super) fn check_workspace_mount_mode(config: &Config) -> Check {
    if config.harness.pi.read_only_workspace {
        Check {
            status: Status::Pass,
            name: "Workspace mount mode",
            detail:
                "harness.pi.read_only_workspace is true; /workspace should be mounted read-only"
                    .to_owned(),
        }
    } else {
        Check {
            status: Status::Pass,
            name: "Workspace mount mode",
            detail:
                "harness.pi.read_only_workspace is false; /workspace will be mounted read-write"
                    .to_owned(),
        }
    }
}

pub(super) fn check_git_identity(config: &Config) -> Check {
    match docker::effective_git_identity(config) {
        Some(identity) => Check {
            status: Status::Pass,
            name: "Git identity",
            detail: format!(
                "{} <{}> from {}; will be injected into the room",
                identity.name, identity.email, identity.source
            ),
        },
        None => Check {
            status: Status::Warn,
            name: "Git identity",
            detail: "no Git user.name/user.email configured or inherited; commits may fall back to container root".to_owned(),
        },
    }
}

pub(super) fn check_host_ssh_agent(agent: &ssh::HostSshAgent) -> Check {
    match agent {
        ssh::HostSshAgent::Ready(_) => Check {
            status: Status::Pass,
            name: "Host SSH agent socket",
            detail: agent.status_detail(),
        },
        ssh::HostSshAgent::MissingEnv => Check {
            status: Status::Warn,
            name: "Host SSH agent socket",
            detail: "SSH_AUTH_SOCK is not set. Git over SSH may not work inside the room."
                .to_owned(),
        },
        ssh::HostSshAgent::MissingPath(_) | ssh::HostSshAgent::NotSocket(_) => Check {
            status: Status::Warn,
            name: "Host SSH agent socket",
            detail: agent.status_detail(),
        },
    }
}

pub(super) fn check_ssh_auth_sock_env() -> Check {
    match env::var("SSH_AUTH_SOCK") {
        Ok(value) if !value.trim().is_empty() => Check {
            status: Status::Pass,
            name: "SSH_AUTH_SOCK env",
            detail: format!("SSH_AUTH_SOCK is set: {value}"),
        },
        _ => Check {
            status: Status::Warn,
            name: "SSH_AUTH_SOCK env",
            detail: "No SSH agent detected. Git over SSH may not work inside the room.".to_owned(),
        },
    }
}
