use std::borrow::Cow;

use anyhow::Result;

use super::{Check, Status};
use crate::{config::Config, docker, harness, ssh};

pub(super) fn check_container_git_identity(probe: &Result<docker::ContainerDoctorProbe>) -> Check {
    match probe {
        Ok(probe) => match &probe.git_identity {
            Some(identity) => Check {
                status: Status::Pass,
                name: "Room Git identity".into(),
                detail: format!(
                    "{} <{}> is available inside the room",
                    identity.name, identity.email
                ),
            },
            None => Check {
                status: Status::Warn,
                name: "Room Git identity".into(),
                detail: "Git identity injection is not active inside the room".to_owned(),
            },
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Room Git identity".into(),
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
                name: "Container SSH_AUTH_SOCK".into(),
                detail: "skipped because no host agent or managed SSH keys are configured"
                    .to_owned(),
            });
        }
        Ok(Some(probe)) => {
            checks.push(if probe.receives_ssh_auth_sock {
                Check {
                    status: Status::Pass,
                    name: "Container SSH_AUTH_SOCK".into(),
                    detail: format!("container receives {}", ssh::CONTAINER_SSH_AUTH_SOCK),
                }
            } else {
                Check {
                    status: Status::Fail,
                    name: "Container SSH_AUTH_SOCK".into(),
                    detail: "container did not receive a usable mounted SSH agent socket"
                        .to_owned(),
                }
            });

            checks.push(if probe.has_ssh_add {
                Check {
                    status: Status::Pass,
                    name: "Container ssh-add".into(),
                    detail: "ssh-add is available inside the room".to_owned(),
                }
            } else {
                Check {
                    status: Status::Fail,
                    name: "Container ssh-add".into(),
                    detail: "ssh-add was not found inside the room".to_owned(),
                }
            });

            checks.push(check_container_ssh_add_result(probe.ssh_add));
        }
        Err(err) => {
            checks.push(Check {
                status: Status::Fail,
                name: "Container SSH_AUTH_SOCK".into(),
                detail: format!("Docker could not mount/check the ssh-agent socket: {err:#}"),
            });
            checks.push(Check {
                status: Status::Warn,
                name: "Container ssh-add".into(),
                detail: "skipped because the container SSH probe failed".to_owned(),
            });
            checks.push(Check {
                status: Status::Warn,
                name: "Container ssh-add -l".into(),
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
            name: "Container ssh-add -l".into(),
            detail: if result.stdout.is_empty() {
                "ssh-add -l succeeded".to_owned()
            } else {
                result.stdout
            },
        }
    } else if result.code == 1 {
        Check {
            status: Status::Warn,
            name: "Container ssh-add -l".into(),
            detail:
                "ssh-agent is reachable but has no loaded identities. Run `ssh-add` on the host."
                    .to_owned(),
        }
    } else {
        Check {
            status: Status::Fail,
            name: "Container ssh-add -l".into(),
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

/// Each mounted state dir is writable inside the room, and the room can reach the
/// harness's provider endpoint (needed for `/login` and normal operation).
pub(super) fn check_container_state_and_network(
    probe: &Result<docker::ContainerDoctorProbe>,
) -> Vec<Check> {
    let mut checks = Vec::new();

    match probe {
        Ok(probe) => {
            for dir in &probe.state_dirs {
                let name: Cow<'static, str> =
                    format!("Container {} state writable", dir.name).into();
                checks.push(if dir.writable {
                    Check {
                        status: Status::Pass,
                        name,
                        detail: format!("{} is writable inside the room", dir.container_path),
                    }
                } else {
                    Check {
                        status: Status::Fail,
                        name,
                        detail: format!("{} is not writable inside the room", dir.container_path),
                    }
                });
            }

            checks.push(if probe.internet_reachable {
                Check {
                    status: Status::Pass,
                    name: "Container internet".into(),
                    detail: format!("container can reach {}", probe.connectivity_url),
                }
            } else {
                Check {
                    status: Status::Warn,
                    name: "Container internet".into(),
                    detail: format!(
                        "container could not reach {}; /login and API calls may fail",
                        probe.connectivity_url
                    ),
                }
            });
        }
        Err(err) => {
            checks.push(Check {
                status: Status::Warn,
                name: "Container state writable".into(),
                detail: format!("could not test state write paths inside the room: {err:#}"),
            });
            checks.push(Check {
                status: Status::Warn,
                name: "Container internet".into(),
                detail: format!("could not test outbound HTTPS from the room: {err:#}"),
            });
        }
    }

    checks
}

/// The configured harness command resolves inside the room and reports a version.
pub(super) fn check_container_harness_command(
    probe: &Result<docker::ContainerDoctorProbe>,
) -> Check {
    match probe {
        Ok(probe) => match (&probe.command_path, &probe.command_version) {
            (Some(path), Some(version)) => Check {
                status: Status::Pass,
                name: "Container harness command".into(),
                detail: format!("{path} ({version})"),
            },
            (Some(path), None) => Check {
                status: Status::Warn,
                name: "Container harness command".into(),
                detail: format!(
                    "{path} found, but `{} --version` reported nothing",
                    probe.command
                ),
            },
            (None, _) => Check {
                status: Status::Fail,
                name: "Container harness command".into(),
                detail: format!("`{}` was not found on PATH inside the room", probe.command),
            },
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container harness command".into(),
            detail: format!("could not locate the harness command inside the room: {err:#}"),
        },
    }
}

pub(super) fn check_container_python(
    config: &Config,
    probe: &Result<docker::ContainerDoctorProbe>,
) -> Check {
    if !docker::environment::python_enabled(config) {
        return Check {
            status: Status::Pass,
            name: "Container Python toolchain".into(),
            detail: "disabled".to_owned(),
        };
    }

    match probe {
        Ok(probe) if probe.python_available => Check {
            status: Status::Pass,
            name: "Container Python toolchain".into(),
            detail: probe
                .python_version
                .clone()
                .unwrap_or_else(|| "python, python3, pip, and venv are available".to_owned()),
        },
        Ok(_) => Check {
            status: Status::Fail,
            name: "Container Python toolchain".into(),
            detail: "python/python3, pip, or venv was not available inside the room".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container Python toolchain".into(),
            detail: format!("could not check Python inside the room: {err:#}"),
        },
    }
}

pub(super) fn check_container_go(
    config: &Config,
    probe: &Result<docker::ContainerDoctorProbe>,
) -> Check {
    if !docker::environment::go_enabled(config) {
        return Check {
            status: Status::Pass,
            name: "Container Go toolchain".into(),
            detail: "disabled".to_owned(),
        };
    }

    match probe {
        Ok(probe) if probe.go_available => Check {
            status: Status::Pass,
            name: "Container Go toolchain".into(),
            detail: probe
                .go_version
                .clone()
                .unwrap_or_else(|| "go and gofmt are available".to_owned()),
        },
        Ok(_) => Check {
            status: Status::Fail,
            name: "Container Go toolchain".into(),
            detail: "go or gofmt was not available inside the room".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container Go toolchain".into(),
            detail: format!("could not check Go inside the room: {err:#}"),
        },
    }
}

pub(super) fn check_container_typescript(
    config: &Config,
    probe: &Result<docker::ContainerDoctorProbe>,
) -> Check {
    if !docker::environment::typescript_enabled(config) {
        return Check {
            status: Status::Pass,
            name: "Container TypeScript toolchain".into(),
            detail: "disabled".to_owned(),
        };
    }

    match probe {
        Ok(probe) if probe.typescript_available => Check {
            status: Status::Pass,
            name: "Container TypeScript toolchain".into(),
            detail: probe
                .tsc_version
                .clone()
                .unwrap_or_else(|| "tsc is available".to_owned()),
        },
        Ok(_) => Check {
            status: Status::Fail,
            name: "Container TypeScript toolchain".into(),
            detail: "tsc was not available inside the room".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container TypeScript toolchain".into(),
            detail: format!("could not check TypeScript inside the room: {err:#}"),
        },
    }
}

/// Pi-only: in-room `npm install -g` updates land in the persisted npm-global
/// prefix, which must precede the baked install on PATH.
pub(super) fn check_container_pi_package(
    probe: &Result<docker::ContainerDoctorProbe>,
) -> Vec<Check> {
    let mut checks = Vec::new();
    let pi_npm_global_path =
        harness::PI.required_state_dir_container_path(harness::PI_NPM_GLOBAL_DIR);
    let pi_npm_global_bin = format!("{pi_npm_global_path}/bin");
    let pi_npm_global_bin_prefix = format!("{pi_npm_global_bin}/");

    checks.push(match probe {
        Ok(probe) if docker::path_contains_entry(&probe.container_path, &pi_npm_global_bin) => {
            Check {
                status: Status::Pass,
                name: "Container npm-global PATH".into(),
                detail: format!("{pi_npm_global_bin} is on PATH inside the room"),
            }
        }
        Ok(probe) => Check {
            status: Status::Fail,
            name: "Container npm-global PATH".into(),
            detail: format!(
                "{pi_npm_global_bin} is not on PATH; persisted Pi updates will not be detected. Container PATH: {}",
                probe.container_path
            ),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container npm-global PATH".into(),
            detail: format!("could not inspect PATH inside the room: {err:#}"),
        },
    });

    checks.push(match probe {
        Ok(probe) => match probe.command_path.as_deref() {
            Some(path)
                if is_in_persistent_npm_global_bin(
                    path,
                    &pi_npm_global_bin,
                    &pi_npm_global_bin_prefix,
                ) =>
            {
                Check {
                    status: Status::Pass,
                    name: "Container Pi package source".into(),
                    detail: format!("persisted user-installed Pi package is active: {path}"),
                }
            }
            Some(path) => Check {
                status: Status::Pass,
                name: "Container Pi package source".into(),
                detail: format!("using baked image Pi package fallback: {path}"),
            },
            None => Check {
                status: Status::Warn,
                name: "Container Pi package source".into(),
                detail: "skipped because pi was not found inside the room".to_owned(),
            },
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container Pi package source".into(),
            detail: format!("could not locate pi inside the room: {err:#}"),
        },
    });

    checks
}

fn is_in_persistent_npm_global_bin(path: &str, bin: &str, bin_prefix: &str) -> bool {
    path == bin || path.starts_with(bin_prefix)
}
