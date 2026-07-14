mod container;
mod host;
mod output;
mod path_checks;
mod runtime;

use anyhow::Result;

use crate::{config::Config, docker, harness, paths::StatePaths, ssh};

use self::{
    container::{
        check_container_git_identity, check_container_go, check_container_login_readiness,
        check_container_pi_package, check_container_python, check_container_ssh,
        check_container_typescript,
    },
    host::{
        check_config_git_section, check_git_identity, check_host_ssh_agent, check_network_mode,
        check_read_only_rootfs_mode, check_ssh_auth_sock_env, check_ssh_configuration,
        check_workspace_mount_mode, check_workspace_risky_mount_policy,
    },
    output::print_checks,
    path_checks::{
        check_dir_writable, check_known_hosts, check_path_dir, check_path_file, check_pi_auth_state,
    },
    runtime::check_compose_runtime_settings,
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
        format!(
            "Docker can run a trivial container with --network {}",
            config.harness.pi.network
        ),
        format!(
            "Docker could not run `hello-world` with `--network {}`.",
            config.harness.pi.network
        ),
    ));

    for (name, path) in [
        ("State root", &state.root),
        ("Workspace", &state.workspace),
        ("Pi harness", &state.pi_root),
        ("Pi config", &state.pi_config),
        ("Pi extensions", &state.pi_extensions),
        ("Pi skills", &state.pi_skills),
        ("Pi sessions", &state.pi_sessions),
        ("Pi npm global", &state.pi_npm_global),
        ("Environment", &state.environment_root),
        ("Cargo home", &state.cargo_home),
        ("SSH directory", &state.ssh_dir),
        ("Cache", &state.cache),
    ] {
        checks.push(check_path_dir(name, path));
    }

    checks.push(check_path_file("Config", &state.config_yaml));
    checks.push(check_config_git_section(&state.config_yaml));
    checks.push(check_network_mode(&config));
    checks.push(check_workspace_risky_mount_policy(&config));
    checks.push(check_workspace_mount_mode(&config));
    checks.push(check_read_only_rootfs_mode(&config));
    checks.push(check_environment_apt_packages(&config));
    checks.push(check_environment_rust(&config));
    checks.push(check_environment_python(&config));
    checks.push(check_environment_go(&config));
    checks.push(check_environment_typescript(&config));
    checks.push(check_known_hosts(&state.known_hosts));
    checks.push(check_dir_writable("Pi config writable", &state.pi_config));
    checks.push(check_dir_writable(
        "Pi sessions writable",
        &state.pi_sessions,
    ));
    checks.push(check_dir_writable(
        "Pi npm global writable",
        &state.pi_npm_global,
    ));
    checks.push(check_dir_writable("Cargo home writable", &state.cargo_home));
    checks.push(check_pi_auth_state(&state.pi_auth_json));

    let compose_ready = match config.resolved_compose_file() {
        Ok(compose_file) => {
            checks.push(check_path_file("Managed Compose file", &compose_file));
            if let Some(project_dir) = compose_file.parent() {
                checks.push(check_path_file(
                    "Managed Pi Dockerfile",
                    &project_dir.join(harness::PI.dockerfile_path),
                ));
            } else {
                checks.push(Check {
                    status: Status::Fail,
                    name: "Managed Pi Dockerfile",
                    detail: "could not determine Compose project directory".to_owned(),
                });
            }
            checks.extend(check_compose_runtime_settings(&compose_file));
            true
        }
        Err(err) => {
            checks.push(Check {
                status: Status::Fail,
                name: "Managed Compose file",
                detail: format!("{err:#}"),
            });
            checks.push(Check {
                status: Status::Fail,
                name: "Managed Pi Dockerfile",
                detail: "skipped because the managed Compose runtime could not be resolved"
                    .to_owned(),
            });
            false
        }
    };

    let image_exists = match docker::image_exists(&config) {
        Ok(true) => {
            checks.push(Check {
                status: Status::Pass,
                name: "Pi image",
                detail: format!("{} exists", docker::pi_runtime_image(&config)),
            });
            true
        }
        Ok(false) => {
            checks.push(Check {
                status: Status::Warn,
                name: "Pi image",
                detail: format!(
                    "{} was not found. Run: vr init --build",
                    docker::pi_runtime_image(&config)
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
    checks.extend(check_ssh_configuration(&config, &host_agent));
    checks.push(check_host_ssh_agent(&host_agent));
    checks.push(check_ssh_auth_sock_env());
    checks.push(check_git_identity(&config));

    if image_exists && compose_ready {
        let container_probe = docker::container_doctor_probe(&config);
        checks.extend(check_container_ssh(&config));
        checks.push(check_container_git_identity(&container_probe));
        checks.extend(check_container_login_readiness(&container_probe));
        checks.push(check_container_python(&config, &container_probe));
        checks.push(check_container_go(&config, &container_probe));
        checks.push(check_container_typescript(&config, &container_probe));
        checks.extend(check_container_pi_package(&container_probe));
    } else if !compose_ready {
        checks.push(Check {
            status: Status::Warn,
            name: "Container checks",
            detail:
                "skipped because the managed Compose runtime could not be resolved. Run: vr init."
                    .to_owned(),
        });
        checks.push(Check {
            status: Status::Warn,
            name: "Room Git identity",
            detail:
                "skipped because the managed Compose runtime could not be resolved. Run: vr init."
                    .to_owned(),
        });
    } else {
        checks.push(Check {
            status: Status::Warn,
            name: "Container SSH checks",
            detail: "skipped because the Pi image is missing. Run: vr init --build".to_owned(),
        });
        checks.push(Check {
            status: Status::Warn,
            name: "Room Git identity",
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

fn check_environment_apt_packages(config: &Config) -> Check {
    let packages = docker::environment_apt_packages(config);
    if packages.is_empty() {
        Check {
            status: Status::Pass,
            name: "Environment apt packages",
            detail: "no extra apt packages configured".to_owned(),
        }
    } else {
        Check {
            status: Status::Pass,
            name: "Environment apt packages",
            detail: format!("configured: {}", packages.join(", ")),
        }
    }
}

fn check_environment_rust(config: &Config) -> Check {
    if !docker::environment_rust_enabled(config) {
        return Check {
            status: Status::Pass,
            name: "Environment Rust toolchain",
            detail: "disabled".to_owned(),
        };
    }

    let components = docker::environment_rust_components(config);
    let component_detail = if components.is_empty() {
        "no extra components".to_owned()
    } else {
        format!("components: {}", components.join(", "))
    };

    Check {
        status: Status::Pass,
        name: "Environment Rust toolchain",
        detail: format!(
            "enabled; toolchain: {}; {component_detail}",
            docker::environment_rust_toolchain(config)
        ),
    }
}

fn check_environment_python(config: &Config) -> Check {
    if docker::environment_python_enabled(config) {
        Check {
            status: Status::Pass,
            name: "Environment Python toolchain",
            detail: "enabled; installs python, python3, pip, and venv".to_owned(),
        }
    } else {
        Check {
            status: Status::Pass,
            name: "Environment Python toolchain",
            detail: "disabled".to_owned(),
        }
    }
}

fn check_environment_go(config: &Config) -> Check {
    if docker::environment_go_enabled(config) {
        Check {
            status: Status::Pass,
            name: "Environment Go toolchain",
            detail: "enabled; installs go and gofmt".to_owned(),
        }
    } else {
        Check {
            status: Status::Pass,
            name: "Environment Go toolchain",
            detail: "disabled".to_owned(),
        }
    }
}

fn check_environment_typescript(config: &Config) -> Check {
    if docker::environment_typescript_enabled(config) {
        Check {
            status: Status::Pass,
            name: "Environment TypeScript toolchain",
            detail: format!(
                "enabled; npm packages: {}",
                docker::environment_typescript_packages(config).join(", ")
            ),
        }
    } else {
        Check {
            status: Status::Pass,
            name: "Environment TypeScript toolchain",
            detail: "disabled".to_owned(),
        }
    }
}

fn command_available(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("-h")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
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
