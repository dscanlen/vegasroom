use std::{env, fs, path::Path};

use anyhow::Result;

use crate::{
    config::Config,
    docker,
    paths::{display_path, StatePaths},
    ssh,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Pass,
    Warn,
    Fail,
}

struct Check {
    status: Status,
    name: &'static str,
    detail: String,
}

pub fn run() -> Result<i32> {
    let config = Config::load_or_default()?;
    let state = StatePaths::default()?;
    let mut checks = Vec::new();

    checks.push(check_bool(
        Status::Fail,
        "Docker binary",
        docker::docker_command_available(),
        "docker is available on PATH",
        "Docker was not found on PATH.",
    ));

    checks.push(check_bool(
        Status::Fail,
        "Docker Compose",
        docker::compose_available(),
        "docker compose is available",
        "Docker Compose was not available via `docker compose`.",
    ));

    checks.push(check_bool(
        Status::Fail,
        "Docker context",
        docker::context_exists(&config),
        format!("Docker context '{}' exists", config.docker.context),
        format!(
            "Docker context '{}' was not found. Create or select a rootless Docker context before running Vegasroom.",
            config.docker.context
        ),
    ));

    checks.push(check_bool(
        Status::Fail,
        "Rootless context usable",
        docker::context_usable(&config),
        format!(
            "Docker context '{}' responded to `docker info`",
            config.docker.context
        ),
        format!(
            "Docker context '{}' did not respond to `docker info`.",
            config.docker.context
        ),
    ));

    checks.push(check_bool(
        Status::Fail,
        "Trivial container",
        docker::can_run_trivial_container(&config),
        "Docker can run a trivial container with host networking",
        "Docker could not run `hello-world` with `--network host`.",
    ));

    for (name, path) in [
        ("State root", &state.root),
        ("Workspace", &state.workspace),
        ("Pi harness", &state.pi_root),
        ("Pi config", &state.pi_config),
        ("Pi extensions", &state.pi_extensions),
        ("Pi skills", &state.pi_skills),
        ("Pi sessions", &state.pi_sessions),
        ("SSH directory", &state.ssh_dir),
        ("Cache", &state.cache),
    ] {
        checks.push(check_path_dir(name, path));
    }

    checks.push(check_path_file("Config", &state.config_yaml));
    checks.push(check_known_hosts(&state.known_hosts));
    checks.push(check_dir_writable("Pi config writable", &state.pi_config));
    checks.push(check_dir_writable(
        "Pi sessions writable",
        &state.pi_sessions,
    ));
    checks.push(check_pi_auth_state(&state.pi_auth_json));

    let compose_file = config.compose_file_path();
    checks.push(check_path_file("Compose file", &compose_file));
    checks.push(check_path_file(
        "Pi Dockerfile",
        Path::new("harness/pi/Dockerfile"),
    ));
    checks.extend(check_compose_runtime_settings(&compose_file));

    let image_exists = match docker::image_exists(&config) {
        Ok(true) => {
            checks.push(Check {
                status: Status::Pass,
                name: "Pi image",
                detail: format!("{} exists", config.harness.pi.image),
            });
            true
        }
        Ok(false) => {
            checks.push(Check {
                status: Status::Warn,
                name: "Pi image",
                detail: format!(
                    "{} was not found. Run: vr init --build",
                    config.harness.pi.image
                ),
            });
            false
        }
        Err(err) => {
            checks.push(Check {
                status: Status::Warn,
                name: "Pi image",
                detail: format!("could not inspect image: {err:#}"),
            });
            false
        }
    };

    let host_agent = ssh::detect_host_agent();
    checks.push(check_host_ssh_agent(&host_agent));
    checks.push(check_ssh_auth_sock_env());

    if image_exists {
        checks.extend(check_container_ssh(&config, &host_agent));
        checks.extend(check_container_login_readiness(&config));
    } else {
        checks.push(Check {
            status: Status::Warn,
            name: "Container SSH checks",
            detail: "skipped because the Pi image is missing. Run: vr init --build".to_owned(),
        });
    }

    print_checks(&checks);

    if checks.iter().any(|check| check.status == Status::Fail) {
        Ok(1)
    } else {
        Ok(0)
    }
}

fn check_bool(
    fail_status: Status,
    name: &'static str,
    passed: bool,
    pass_detail: impl Into<String>,
    fail_detail: impl Into<String>,
) -> Check {
    if passed {
        Check {
            status: Status::Pass,
            name,
            detail: pass_detail.into(),
        }
    } else {
        Check {
            status: fail_status,
            name,
            detail: fail_detail.into(),
        }
    }
}

fn check_path_dir(name: &'static str, path: &Path) -> Check {
    if path.is_dir() {
        Check {
            status: Status::Pass,
            name,
            detail: format!("{} exists", display_path(path)),
        }
    } else if path.exists() {
        Check {
            status: Status::Fail,
            name,
            detail: format!(
                "expected directory path exists as a file: {}",
                display_path(path)
            ),
        }
    } else {
        Check {
            status: Status::Fail,
            name,
            detail: format!("missing directory: {}. Run: vr init", display_path(path)),
        }
    }
}

fn check_path_file(name: &'static str, path: &Path) -> Check {
    if path.is_file() {
        Check {
            status: Status::Pass,
            name,
            detail: format!("{} exists", display_path(path)),
        }
    } else if path.exists() {
        Check {
            status: Status::Fail,
            name,
            detail: format!(
                "expected file path exists as a directory: {}",
                display_path(path)
            ),
        }
    } else {
        Check {
            status: Status::Fail,
            name,
            detail: format!("missing file: {}. Run: vr init", display_path(path)),
        }
    }
}

fn check_known_hosts(path: &Path) -> Check {
    if path.is_file() {
        Check {
            status: Status::Pass,
            name: "known_hosts",
            detail: format!("{} exists", display_path(path)),
        }
    } else if path.exists() {
        Check {
            status: Status::Fail,
            name: "known_hosts",
            detail: format!(
                "expected known_hosts to be a file, but path exists as a directory: {}",
                display_path(path)
            ),
        }
    } else if path.parent().map(|parent| parent.is_dir()).unwrap_or(false) {
        Check {
            status: Status::Warn,
            name: "known_hosts",
            detail: format!(
                "{} is missing but can be created by `vr init`",
                display_path(path)
            ),
        }
    } else {
        Check {
            status: Status::Fail,
            name: "known_hosts",
            detail: format!("parent SSH directory is missing for {}", display_path(path)),
        }
    }
}

fn check_dir_writable(name: &'static str, path: &Path) -> Check {
    if !path.is_dir() {
        return check_path_dir(name, path);
    }

    let test_file = path.join(".vr-doctor-write-test");
    match fs::write(&test_file, "doctor\n") {
        Ok(()) => {
            let _ = fs::remove_file(&test_file);
            Check {
                status: Status::Pass,
                name,
                detail: format!("{} is writable", display_path(path)),
            }
        }
        Err(err) => Check {
            status: Status::Fail,
            name,
            detail: format!("{} is not writable: {err}", display_path(path)),
        },
    }
}

fn check_pi_auth_state(path: &Path) -> Check {
    if path.is_file() {
        Check {
            status: Status::Pass,
            name: "Pi auth state",
            detail: format!("{} exists", display_path(path)),
        }
    } else if path.exists() {
        Check {
            status: Status::Fail,
            name: "Pi auth state",
            detail: format!(
                "expected Pi auth state to be a file, but path exists as a directory: {}",
                display_path(path)
            ),
        }
    } else {
        Check {
            status: Status::Warn,
            name: "Pi auth state",
            detail: format!(
                "{} not found. Run `cargo run -- pi`, then use Pi `/login`.",
                display_path(path)
            ),
        }
    }
}

fn check_host_ssh_agent(agent: &ssh::HostSshAgent) -> Check {
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

fn check_ssh_auth_sock_env() -> Check {
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

fn check_container_ssh(config: &Config, agent: &ssh::HostSshAgent) -> Vec<Check> {
    let mut checks = Vec::new();

    if !agent.is_ready() {
        checks.push(Check {
            status: Status::Warn,
            name: "Container SSH_AUTH_SOCK",
            detail: "skipped because no usable host SSH agent socket was detected".to_owned(),
        });
        return checks;
    }

    checks.push(match docker::container_receives_ssh_auth_sock(config) {
        Ok(Some(true)) => Check {
            status: Status::Pass,
            name: "Container SSH_AUTH_SOCK",
            detail: format!("container receives {}", ssh::CONTAINER_SSH_AUTH_SOCK),
        },
        Ok(Some(false)) => Check {
            status: Status::Fail,
            name: "Container SSH_AUTH_SOCK",
            detail: "container did not receive a usable mounted SSH agent socket".to_owned(),
        },
        Ok(None) => Check {
            status: Status::Warn,
            name: "Container SSH_AUTH_SOCK",
            detail: "skipped because no usable host SSH agent socket was detected".to_owned(),
        },
        Err(err) => Check {
            status: Status::Fail,
            name: "Container SSH_AUTH_SOCK",
            detail: format!("Docker could not mount/check the ssh-agent socket: {err:#}"),
        },
    });

    checks.push(match docker::container_has_ssh_add(config) {
        Ok(Some(true)) => Check {
            status: Status::Pass,
            name: "Container ssh-add",
            detail: "ssh-add is available inside the room".to_owned(),
        },
        Ok(Some(false)) => Check {
            status: Status::Fail,
            name: "Container ssh-add",
            detail: "ssh-add was not found inside the room".to_owned(),
        },
        Ok(None) => Check {
            status: Status::Warn,
            name: "Container ssh-add",
            detail: "skipped because no usable host SSH agent socket was detected".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container ssh-add",
            detail: format!("could not check ssh-add inside the room: {err:#}"),
        },
    });

    checks.push(match docker::container_ssh_add_l(config) {
        Ok(Some(result)) if result.code == 0 => Check {
            status: Status::Pass,
            name: "Container ssh-add -l",
            detail: if result.stdout.is_empty() {
                "ssh-add -l succeeded".to_owned()
            } else {
                result.stdout
            },
        },
        Ok(Some(result)) if result.code == 1 => Check {
            status: Status::Warn,
            name: "Container ssh-add -l",
            detail:
                "ssh-agent is reachable but has no loaded identities. Run `ssh-add` on the host."
                    .to_owned(),
        },
        Ok(Some(result)) => Check {
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
        },
        Ok(None) => Check {
            status: Status::Warn,
            name: "Container ssh-add -l",
            detail: "skipped because no usable host SSH agent socket was detected".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container ssh-add -l",
            detail: format!("could not run ssh-add -l inside the room: {err:#}"),
        },
    });

    checks
}

fn check_container_login_readiness(config: &Config) -> Vec<Check> {
    let mut checks = Vec::new();

    checks.push(match docker::container_pi_config_writable(config) {
        Ok(true) => Check {
            status: Status::Pass,
            name: "Container Pi config writable",
            detail: "/home/agent/.pi/agent is writable inside the room".to_owned(),
        },
        Ok(false) => Check {
            status: Status::Fail,
            name: "Container Pi config writable",
            detail: "/home/agent/.pi/agent is not writable inside the room".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container Pi config writable",
            detail: format!("could not test Pi config write path inside the room: {err:#}"),
        },
    });

    checks.push(match docker::container_pi_sessions_writable(config) {
        Ok(true) => Check {
            status: Status::Pass,
            name: "Container Pi sessions writable",
            detail: "/home/agent/.pi/sessions is writable inside the room".to_owned(),
        },
        Ok(false) => Check {
            status: Status::Fail,
            name: "Container Pi sessions writable",
            detail: "/home/agent/.pi/sessions is not writable inside the room".to_owned(),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Container Pi sessions writable",
            detail: format!("could not test Pi session write path inside the room: {err:#}"),
        },
    });

    checks.push(match docker::container_can_reach_internet(config) {
        Ok(true) => Check {
            status: Status::Pass,
            name: "Container internet",
            detail: "container can reach https://pi.dev".to_owned(),
        },
        Ok(false) => Check {
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

fn check_compose_runtime_settings(compose_file: &Path) -> Vec<Check> {
    let mut checks = Vec::new();
    let Ok(contents) = fs::read_to_string(compose_file) else {
        return checks;
    };

    checks.push(check_bool(
        Status::Warn,
        "Compose build network",
        contents.contains("network: ${VR_PI_BUILD_NETWORK:-host}")
            || contents.contains("network: host"),
        "build.network host fallback is present",
        "build.network host fallback was not found in compose.yaml",
    ));

    checks.push(check_bool(
        Status::Warn,
        "Compose runtime network",
        contents.contains("network_mode: ${VR_PI_NETWORK_MODE:-host}")
            || contents.contains("network_mode: host"),
        "network_mode host fallback is present",
        "network_mode host fallback was not found in compose.yaml",
    ));

    checks.push(check_bool(
        Status::Warn,
        "Container user",
        contents.contains("user: \"0:0\"")
            || contents.contains("user: '0:0'")
            || contents.contains("user: 0:0"),
        "container-root runtime is preserved",
        "container-root runtime setting was not found in compose.yaml",
    ));

    checks.push(check_bool(
        Status::Warn,
        "SSH directory mount model",
        contents.contains(".vegasroom/ssh")
            && contents.contains("target: /home/agent/.ssh")
            && contents.contains("target: /root/.ssh"),
        "SSH directory mount is preserved for Pi HOME and root SSH",
        "SSH directory mount was not found for both /home/agent/.ssh and /root/.ssh in compose.yaml",
    ));

    checks.push(Check {
        status: Status::Pass,
        name: "SSH agent mount model",
        detail: "ssh-agent socket mount is generated dynamically when SSH_AUTH_SOCK is usable"
            .to_owned(),
    });

    checks.push(check_bool(
        Status::Warn,
        "Login browser opener",
        contents.contains("BROWSER: echo"),
        "BROWSER=echo is set so Pi browser-login URLs are printed for host-browser use",
        "BROWSER=echo was not found in compose.yaml; Pi may try to open a browser inside the container",
    ));

    checks
}

fn print_checks(checks: &[Check]) {
    for check in checks {
        let label = match check.status {
            Status::Pass => "PASS",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
        };
        println!("{label}: {} - {}", check.name, check.detail);
    }

    let pass = checks
        .iter()
        .filter(|check| check.status == Status::Pass)
        .count();
    let warn = checks
        .iter()
        .filter(|check| check.status == Status::Warn)
        .count();
    let fail = checks
        .iter()
        .filter(|check| check.status == Status::Fail)
        .count();

    println!("\nSummary: {pass} PASS, {warn} WARN, {fail} FAIL");
}
