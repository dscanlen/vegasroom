use std::borrow::Cow;
mod container;
mod host;
mod output;
mod path_checks;
mod runtime;

use anyhow::Result;

use crate::{config::Config, docker, harness, paths::StatePaths, ssh};

use self::{
    container::{
        check_container_git_identity, check_container_go, check_container_harness_command,
        check_container_pi_package, check_container_python, check_container_ssh,
        check_container_state_and_network, check_container_typescript,
    },
    host::{
        check_config_git_section, check_git_identity, check_host_ssh_agent, check_network_mode,
        check_read_only_rootfs_mode, check_ssh_auth_sock_env, check_ssh_configuration,
        check_workspace_mount_mode, check_workspace_risky_mount_policy,
    },
    output::print_checks,
    path_checks::{
        check_dir_writable, check_harness_auth_state, check_known_hosts, check_path_dir,
        check_path_file, check_private_dir_permissions, check_private_file_permissions,
        host_env_is_set,
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
    name: Cow<'static, str>,
    detail: String,
}

pub fn run() -> Result<i32> {
    let config = Config::load_or_default()?;
    let state = StatePaths::default()?;
    let mut checks = Vec::new();

    checks.push(check_config_semantics(&config));

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
        ("Harness state", &state.harness_root),
        ("Environment", &state.environment_root),
        ("Cargo home", &state.cargo_home),
        ("SSH directory", &state.ssh_dir),
        ("Cache", &state.cache),
    ] {
        checks.push(check_path_dir(name, path));
    }

    checks.push(check_path_file("Config", &state.config_yaml));
    for path in state.private_dirs() {
        checks.push(check_private_dir_permissions(&path));
    }
    for path in state.private_files() {
        checks.push(check_private_file_permissions(&path));
    }
    checks.push(check_config_git_section(&state.config_yaml));
    checks.push(check_workspace_risky_mount_policy(&config));
    checks.push(check_environment_apt_packages(&config));
    checks.push(check_environment_rust(&config));
    checks.push(check_environment_python(&config));
    checks.push(check_environment_go(&config));
    checks.push(check_environment_typescript(&config));
    checks.push(check_base_image_freshness(&config, &state));
    checks.push(check_known_hosts(&state.known_hosts));
    checks.push(check_dir_writable("Cargo home writable", &state.cargo_home));

    let compose_file = match config.resolved_compose_file() {
        Ok(compose_file) => {
            checks.push(check_path_file("Managed Compose file", &compose_file));
            checks.extend(check_compose_runtime_settings(&compose_file));
            Some(compose_file)
        }
        Err(err) => {
            checks.push(Check {
                status: Status::Fail,
                name: "Managed Compose file".into(),
                detail: format!("{err:#}"),
            });
            None
        }
    };

    let host_agent = ssh::detect_host_agent();
    checks.extend(check_ssh_configuration(&config, &host_agent));
    checks.push(check_host_ssh_agent(&host_agent));
    checks.push(check_ssh_auth_sock_env());
    checks.push(check_git_identity(&config));

    // SSH forwarding is harness-independent; probe it once in the default harness room.
    let default_harness = harness::HARNESSES[0];
    let default_image_ready = matches!(docker::image_exists(&config, default_harness), Ok(true));
    if compose_file.is_some() && default_image_ready {
        checks.extend(check_container_ssh(&config));
    } else {
        checks.push(Check {
            status: Status::Warn,
            name: "Container SSH checks".into(),
            detail: format!(
                "skipped because the managed Compose runtime or the {} image is not ready. Run: vr init --build",
                default_harness.display_name
            ),
        });
    }

    for descriptor in harness::HARNESSES {
        let harness_checks = check_harness(&config, &state, descriptor, compose_file.as_deref());
        checks.extend(scoped(descriptor, harness_checks));
    }

    print_checks(&checks);

    if checks.iter().any(|check| check.status == Status::Fail) {
        Ok(1)
    } else {
        Ok(0)
    }
}

/// Host- and container-side checks for one harness. Names are scoped by [`scoped`].
fn check_harness(
    config: &Config,
    state: &StatePaths,
    descriptor: &harness::HarnessDescriptor,
    compose_file: Option<&std::path::Path>,
) -> Vec<Check> {
    let mut checks = Vec::new();
    let instance = config.harness.instance(descriptor);

    checks.push(check_path_dir(
        "State root",
        &state.harness_state_root(descriptor),
    ));
    for dir in descriptor.state_dirs {
        let path = state.harness_state_dir(descriptor, dir.name);
        checks.push(check_path_dir(format!("State {}", dir.name), &path));
        checks.push(check_dir_writable(
            format!("State {} writable", dir.name),
            &path,
        ));
    }
    checks.push(check_harness_auth_state(
        &state.harness_auth_path(descriptor),
        descriptor,
        host_env_is_set,
    ));
    checks.push(check_network_mode(descriptor, instance));
    checks.push(check_workspace_mount_mode(descriptor, instance));
    checks.push(check_read_only_rootfs_mode(descriptor, instance));

    let Some(compose_file) = compose_file else {
        checks.push(Check {
            status: Status::Warn,
            name: "Container checks".into(),
            detail:
                "skipped because the managed Compose runtime could not be resolved. Run: vr init."
                    .to_owned(),
        });
        return checks;
    };

    match compose_file.parent() {
        Some(project_dir) => checks.push(check_path_file(
            "Managed Dockerfile",
            &project_dir.join(descriptor.dockerfile_path),
        )),
        None => checks.push(Check {
            status: Status::Fail,
            name: "Managed Dockerfile".into(),
            detail: "could not determine Compose project directory".to_owned(),
        }),
    }

    let image = docker::runtime_image(config, descriptor);
    let image_ready = match docker::image_exists(config, descriptor) {
        Ok(true) => {
            checks.push(Check {
                status: Status::Pass,
                name: "Image".into(),
                detail: format!("{image} exists"),
            });
            true
        }
        Ok(false) => {
            checks.push(Check {
                status: Status::Warn,
                name: "Image".into(),
                detail: format!("{image} was not found. Run: vr init --build"),
            });
            false
        }
        Err(err) => {
            checks.push(Check {
                status: Status::Warn,
                name: "Image".into(),
                detail: format!("could not inspect image {image}: {err:#}"),
            });
            false
        }
    };

    if !image_ready {
        checks.push(Check {
            status: Status::Warn,
            name: "Container checks".into(),
            detail: format!(
                "skipped because the {} image is missing. Run: vr init --build",
                descriptor.display_name
            ),
        });
        return checks;
    }

    let probe = docker::container_doctor_probe(config, descriptor);
    checks.push(check_container_harness_command(&probe));
    checks.extend(check_container_state_and_network(&probe));
    checks.push(check_container_git_identity(&probe));
    checks.push(check_container_python(config, &probe));
    checks.push(check_container_go(config, &probe));
    checks.push(check_container_typescript(config, &probe));
    if descriptor.id == harness::PI.id {
        checks.extend(check_container_pi_package(&probe));
    }

    checks
}

/// Prefix each check name with the harness display name, e.g. `Claude Code: Image`.
fn scoped(descriptor: &harness::HarnessDescriptor, checks: Vec<Check>) -> Vec<Check> {
    checks
        .into_iter()
        .map(|check| Check {
            name: format!("{}: {}", descriptor.display_name, check.name).into(),
            ..check
        })
        .collect()
}

fn check_environment_apt_packages(config: &Config) -> Check {
    let packages = docker::environment::packages(config);
    if packages.is_empty() {
        Check {
            status: Status::Pass,
            name: "Environment apt packages".into(),
            detail: "no extra apt packages configured".to_owned(),
        }
    } else {
        Check {
            status: Status::Pass,
            name: "Environment apt packages".into(),
            detail: format!("configured: {}", packages.join(", ")),
        }
    }
}

fn check_environment_rust(config: &Config) -> Check {
    if !docker::environment::rust_enabled(config) {
        return Check {
            status: Status::Pass,
            name: "Environment Rust toolchain".into(),
            detail: "disabled".to_owned(),
        };
    }

    let components = docker::environment::rust_components(config);
    let component_detail = if components.is_empty() {
        "no extra components".to_owned()
    } else {
        format!("components: {}", components.join(", "))
    };

    Check {
        status: Status::Pass,
        name: "Environment Rust toolchain".into(),
        detail: format!(
            "enabled; toolchain: {}; {component_detail}",
            docker::environment::rust_toolchain(config)
        ),
    }
}

fn check_environment_python(config: &Config) -> Check {
    if docker::environment::python_enabled(config) {
        Check {
            status: Status::Pass,
            name: "Environment Python toolchain".into(),
            detail: "enabled; installs python, python3, pip, and venv".to_owned(),
        }
    } else {
        Check {
            status: Status::Pass,
            name: "Environment Python toolchain".into(),
            detail: "disabled".to_owned(),
        }
    }
}

fn check_environment_go(config: &Config) -> Check {
    if docker::environment::go_enabled(config) {
        Check {
            status: Status::Pass,
            name: "Environment Go toolchain".into(),
            detail: "enabled; installs go and gofmt".to_owned(),
        }
    } else {
        Check {
            status: Status::Pass,
            name: "Environment Go toolchain".into(),
            detail: "disabled".to_owned(),
        }
    }
}

fn check_environment_typescript(config: &Config) -> Check {
    if docker::environment::typescript_enabled(config) {
        Check {
            status: Status::Pass,
            name: "Environment TypeScript toolchain".into(),
            detail: format!(
                "enabled; npm packages: {}",
                docker::environment::typescript_packages(config).join(", ")
            ),
        }
    } else {
        Check {
            status: Status::Pass,
            name: "Environment TypeScript toolchain".into(),
            detail: "disabled".to_owned(),
        }
    }
}

fn check_base_image_freshness(config: &Config, state: &StatePaths) -> Check {
    if docker::environment_image_stale(config, state) {
        Check {
            status: Status::Warn,
            name: "Base image freshness".into(),
            detail: format!(
                "{} is out of date for the current environment profile (or has not been built). Run: vr init --build",
                config.environment.base_image
            ),
        }
    } else {
        Check {
            status: Status::Pass,
            name: "Base image freshness".into(),
            detail: format!(
                "{} was built from the current environment profile",
                config.environment.base_image
            ),
        }
    }
}

fn check_config_semantics(config: &Config) -> Check {
    match config.validate_semantics() {
        Ok(()) => Check {
            status: Status::Pass,
            name: "Config semantic validation".into(),
            detail: "configuration values are semantically valid".to_owned(),
        },
        Err(err) => Check {
            status: Status::Fail,
            name: "Config semantic validation".into(),
            detail: format!("{err:#}"),
        },
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
    name: impl Into<Cow<'static, str>>,
    passed: bool,
    pass_detail: impl Into<String>,
    fail_detail: impl Into<String>,
) -> Check {
    let name: Cow<'static, str> = name.into();
    if passed {
        Check {
            status: Status::Pass,
            name: name.clone(),
            detail: pass_detail.into(),
        }
    } else {
        Check {
            status: fail_status,
            name: name.clone(),
            detail: fail_detail.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::unique_temp_dir;

    #[test]
    fn scoped_prefixes_check_names_with_harness_display_name() {
        let checks = scoped(
            &harness::CLAUDE,
            vec![Check {
                status: Status::Warn,
                name: "Image".into(),
                detail: "missing".to_owned(),
            }],
        );

        assert_eq!(checks[0].name, "Claude Code: Image");
        assert_eq!(checks[0].status, Status::Warn);
        assert_eq!(checks[0].detail, "missing");
    }

    #[test]
    fn harness_checks_without_compose_cover_state_and_auth_then_skip_container() {
        let state = StatePaths::from_root(unique_temp_dir("doctor-harness").join(".vegasroom"));
        state.ensure().unwrap();
        let config = Config::default();

        for descriptor in harness::HARNESSES {
            let checks = scoped(descriptor, check_harness(&config, &state, descriptor, None));
            let names: Vec<&str> = checks.iter().map(|c| c.name.as_ref()).collect();
            let prefix = format!("{}: ", descriptor.display_name);

            assert!(names.iter().all(|name| name.starts_with(&prefix)));
            for dir in descriptor.state_dirs {
                assert!(names.contains(&format!("{prefix}State {} writable", dir.name).as_str()));
            }
            assert!(names.contains(&format!("{prefix}Auth state").as_str()));
            let skipped = checks
                .iter()
                .find(|c| c.name == format!("{prefix}Container checks"))
                .expect("container checks should be reported as skipped");
            assert_eq!(skipped.status, Status::Warn);
            assert!(!checks.iter().any(|c| c.status == Status::Fail));
        }

        let _ = std::fs::remove_dir_all(state.root);
    }
}
