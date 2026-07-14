use anyhow::Result;

use super::{Check, Status};
use crate::{config::Config, docker, harness, ssh};

pub(super) fn check_container_git_identity(probe: &Result<docker::ContainerDoctorProbe>) -> Check {
    match probe {
        Ok(probe) => match &probe.git_identity {
            Some(identity) => Check {
                status: Status::Pass,
                name: "Room Git identity",
                detail: format!(
                    "{} <{}> is available inside the room",
                    identity.name, identity.email
                ),
            },
            None => Check {
                status: Status::Warn,
                name: "Room Git identity",
                detail: "Git identity injection is not active inside the room".to_owned(),
            },
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Room Git identity",
            detail: format!("could not check Git identity inside the room: {err:#}"),
        },
    }
}

pub(super) fn check_container_ssh(config: &Config) -> Vec<Check> {
    let mut checks = Vec::new();

    match docker::container_ssh_doctor_probe(config) {
        Ok(None) => {
            checks.push(Check {
                status: Status::Warn,
                name: "Container SSH_AUTH_SOCK",
                detail: "skipped because no host agent or managed SSH keys are configured"
                    .to_owned(),
            });
        }
        Ok(Some(probe)) => {
            checks.push(if probe.receives_ssh_auth_sock {
                Check {
                    status: Status::Pass,
                    name: "Container SSH_AUTH_SOCK",
                    detail: format!("container receives {}", ssh::CONTAINER_SSH_AUTH_SOCK),
                }
            } else {
                Check {
                    status: Status::Fail,
                    name: "Container SSH_AUTH_SOCK",
                    detail: "container did not receive a usable mounted SSH agent socket"
                        .to_owned(),
                }
            });

            checks.push(if probe.has_ssh_add {
                Check {
                    status: Status::Pass,
                    name: "Container ssh-add",
                    detail: "ssh-add is available inside the room".to_owned(),
                }
            } else {
                Check {
                    status: Status::Fail,
                    name: "Container ssh-add",
                    detail: "ssh-add was not found inside the room".to_owned(),
                }
            });

            checks.push(check_container_ssh_add_result(probe.ssh_add));
        }
        Err(err) => {
            checks.push(Check {
                status: Status::Fail,
                name: "Container SSH_AUTH_SOCK",
                detail: format!("Docker could not mount/check the ssh-agent socket: {err:#}"),
            });
            checks.push(Check {
                status: Status::Warn,
                name: "Container ssh-add",
                detail: "skipped because the container SSH probe failed".to_owned(),
            });
            checks.push(Check {
                status: Status::Warn,
                name: "Container ssh-add -l",
                detail: "skipped because the container SSH probe failed".to_owned(),
            });
        }
    }

    checks
}

pub(super) fn check_container_ssh_add_result(result: docker::SshAddCheck) -> Check {
    if result.code == 0 {
        Check {
            status: Status::Pass,
            name: "Container ssh-add -l",
            detail: if result.stdout.is_empty() {
                "ssh-add -l succeeded".to_owned()
            } else {
                result.stdout
            },
        }
    } else if result.code == 1 {
        Check {
            status: Status::Warn,
            name: "Container ssh-add -l",
            detail:
                "ssh-agent is reachable but has no loaded identities. Run `ssh-add` on the host."
                    .to_owned(),
        }
    } else {
        Check {
            status: Status::Fail,
            name: "Container ssh-add -l",
            detail: format!(
                "ssh-add -l failed with code {}: {}{}{}",
                result.code,
                result.stdout,
                if result.stdout.is_empty() || result.stderr.is_empty() {
                    ""
                } else {
                    " | "
                },
                result.stderr
            ),
        }
    }
}

pub(super) fn check_container_login_readiness(
    probe: &Result<docker::ContainerDoctorProbe>,
) -> Vec<Check> {
    let mut checks = Vec::new();
    let pi_config_path = harness::PI.required_state_dir_container_path(harness::PI_CONFIG_DIR);
    let pi_sessions_path = harness::PI.required_state_dir_container_path(harness::PI_SESSIONS_DIR);

    checks.push(match probe {
        Ok(probe) if probe.pi_config_writable => Check {
            status: Status::Pass,
            name: "Container Pi config writable",
            detail: format!("{pi_config_path} is writable inside the room"),
        },
        Ok(_) => Check {
            status: Status::Fail,
            name: "Container Pi config writable",
            detail: format!("{pi_config_path} is not writable inside the room"),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container Pi config writable",
            detail: format!("could not test Pi config write path inside the room: {err:#}"),
        },
    });

    checks.push(match probe {
        Ok(probe) if probe.pi_sessions_writable => Check {
            status: Status::Pass,
            name: "Container Pi sessions writable",
            detail: format!("{pi_sessions_path} is writable inside the room"),
        },
        Ok(_) => Check {
            status: Status::Fail,
            name: "Container Pi sessions writable",
            detail: format!("{pi_sessions_path} is not writable inside the room"),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container Pi sessions writable",
            detail: format!("could not test Pi session write path inside the room: {err:#}"),
        },
    });

    checks.push(match probe {
        Ok(probe) if probe.internet_reachable => Check {
            status: Status::Pass,
            name: "Container internet",
            detail: "container can reach https://pi.dev".to_owned(),
        },
        Ok(_) => Check {
            status: Status::Warn,
            name: "Container internet",
            detail: "container could not reach https://pi.dev; Pi login may fail".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container internet",
            detail: format!("could not test outbound HTTPS from the room: {err:#}"),
        },
    });

    checks
}

pub(super) fn check_container_python(
    config: &Config,
    probe: &Result<docker::ContainerDoctorProbe>,
) -> Check {
    if !docker::environment_python_enabled(config) {
        return Check {
            status: Status::Pass,
            name: "Container Python toolchain",
            detail: "disabled".to_owned(),
        };
    }

    match probe {
        Ok(probe) if probe.python_available => Check {
            status: Status::Pass,
            name: "Container Python toolchain",
            detail: probe
                .python_version
                .clone()
                .unwrap_or_else(|| "python, python3, pip, and venv are available".to_owned()),
        },
        Ok(_) => Check {
            status: Status::Fail,
            name: "Container Python toolchain",
            detail: "python/python3, pip, or venv was not available inside the room".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container Python toolchain",
            detail: format!("could not check Python inside the room: {err:#}"),
        },
    }
}

pub(super) fn check_container_go(
    config: &Config,
    probe: &Result<docker::ContainerDoctorProbe>,
) -> Check {
    if !docker::environment_go_enabled(config) {
        return Check {
            status: Status::Pass,
            name: "Container Go toolchain",
            detail: "disabled".to_owned(),
        };
    }

    match probe {
        Ok(probe) if probe.go_available => Check {
            status: Status::Pass,
            name: "Container Go toolchain",
            detail: probe
                .go_version
                .clone()
                .unwrap_or_else(|| "go and gofmt are available".to_owned()),
        },
        Ok(_) => Check {
            status: Status::Fail,
            name: "Container Go toolchain",
            detail: "go or gofmt was not available inside the room".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container Go toolchain",
            detail: format!("could not check Go inside the room: {err:#}"),
        },
    }
}

pub(super) fn check_container_typescript(
    config: &Config,
    probe: &Result<docker::ContainerDoctorProbe>,
) -> Check {
    if !docker::environment_typescript_enabled(config) {
        return Check {
            status: Status::Pass,
            name: "Container TypeScript toolchain",
            detail: "disabled".to_owned(),
        };
    }

    match probe {
        Ok(probe) if probe.typescript_available => Check {
            status: Status::Pass,
            name: "Container TypeScript toolchain",
            detail: probe
                .tsc_version
                .clone()
                .unwrap_or_else(|| "tsc is available".to_owned()),
        },
        Ok(_) => Check {
            status: Status::Fail,
            name: "Container TypeScript toolchain",
            detail: "tsc was not available inside the room".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container TypeScript toolchain",
            detail: format!("could not check TypeScript inside the room: {err:#}"),
        },
    }
}

pub(super) fn check_container_pi_package(
    probe: &Result<docker::ContainerDoctorProbe>,
) -> Vec<Check> {
    let mut checks = Vec::new();
    let pi_npm_global_path =
        harness::PI.required_state_dir_container_path(harness::PI_NPM_GLOBAL_DIR);
    let pi_npm_global_bin = format!("{pi_npm_global_path}/bin");
    let pi_npm_global_bin_prefix = format!("{pi_npm_global_bin}/");

    checks.push(match probe {
        Ok(probe) if probe.pi_npm_global_writable => Check {
            status: Status::Pass,
            name: "Container Pi npm global writable",
            detail: format!("{pi_npm_global_path} is writable inside the room"),
        },
        Ok(_) => Check {
            status: Status::Fail,
            name: "Container Pi npm global writable",
            detail: format!(
                "{pi_npm_global_path} is not writable inside the room; in-room Pi npm updates will not persist"
            ),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container Pi npm global writable",
            detail: format!("could not test Pi npm global write path inside the room: {err:#}"),
        },
    });

    checks.push(match probe {
        Ok(probe) if probe.npm_global_bin_on_path => Check {
            status: Status::Pass,
            name: "Container npm-global PATH",
            detail: format!("{pi_npm_global_bin} is PATH state for persisted Pi updates"),
        },
        Ok(probe) => Check {
            status: Status::Fail,
            name: "Container npm-global PATH",
            detail: format!(
                "{pi_npm_global_bin} is not on PATH; persisted Pi updates will not be detected. Container PATH: {}",
                probe.container_path
            ),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container npm-global PATH",
            detail: format!("could not inspect PATH inside the room: {err:#}"),
        },
    });

    checks.push(match probe {
        Ok(probe) => match probe.pi_command_path.as_deref() {
            Some(path) => {
                if is_in_persistent_npm_global_bin(
                    path,
                    &pi_npm_global_bin,
                    &pi_npm_global_bin_prefix,
                ) {
                    Check {
                        status: Status::Pass,
                        name: "Container Pi command",
                        detail: format!("persisted user-installed Pi package is active: {path}"),
                    }
                } else {
                    Check {
                        status: Status::Pass,
                        name: "Container Pi command",
                        detail: format!("using baked image Pi package fallback: {path}"),
                    }
                }
            }
            None => Check {
                status: Status::Fail,
                name: "Container Pi command",
                detail: "pi command was not found inside the room".to_owned(),
            },
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container Pi command",
            detail: format!("could not locate pi inside the room: {err:#}"),
        },
    });

    checks
}

fn is_in_persistent_npm_global_bin(path: &str, bin: &str, bin_prefix: &str) -> bool {
    path == bin || path.starts_with(bin_prefix)
}
