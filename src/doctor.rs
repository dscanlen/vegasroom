use std::{env, fs, path::Path};

use anyhow::Result;

use crate::{config::Config, docker, paths::{display_path, StatePaths}};

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
        format!("Docker context '{}' responded to `docker info`", config.docker.context),
        format!("Docker context '{}' did not respond to `docker info`.", config.docker.context),
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

    let compose_file = config.compose_file_path();
    checks.push(check_path_file("Compose file", &compose_file));
    checks.extend(check_compose_m1_settings(&compose_file));

    checks.push(match docker::image_exists(&config) {
        Ok(true) => Check {
            status: Status::Pass,
            name: "Pi image",
            detail: format!("{} exists", config.harness.pi.image),
        },
        Ok(false) => Check {
            status: Status::Warn,
            name: "Pi image",
            detail: format!("{} was not found. Run: vr init --build", config.harness.pi.image),
        },
        Err(err) => Check {
            status: Status::Warn,
            name: "Pi image",
            detail: format!("could not inspect image: {err:#}"),
        },
    });

    checks.push(check_ssh_auth_sock());

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
            detail: format!("expected directory path exists as a file: {}", display_path(path)),
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
            detail: format!("expected file path exists as a directory: {}", display_path(path)),
        }
    } else {
        Check {
            status: Status::Fail,
            name,
            detail: format!("missing file: {}. Run: vr init", display_path(path)),
        }
    }
}

fn check_ssh_auth_sock() -> Check {
    match env::var("SSH_AUTH_SOCK") {
        Ok(value) if !value.trim().is_empty() => Check {
            status: Status::Pass,
            name: "SSH agent",
            detail: format!("SSH_AUTH_SOCK is set: {value}"),
        },
        _ => Check {
            status: Status::Warn,
            name: "SSH agent",
            detail: "No SSH agent detected. Git over SSH may not work inside the room.".to_owned(),
        },
    }
}

fn check_compose_m1_settings(compose_file: &Path) -> Vec<Check> {
    let mut checks = Vec::new();
    let Ok(contents) = fs::read_to_string(compose_file) else {
        return checks;
    };

    checks.push(check_bool(
        Status::Warn,
        "Compose build network",
        contents.contains("network: ${VR_PI_BUILD_NETWORK:-host}") || contents.contains("network: host"),
        "build.network host fallback is present",
        "build.network host fallback was not found in compose.yaml",
    ));

    checks.push(check_bool(
        Status::Warn,
        "Compose runtime network",
        contents.contains("network_mode: ${VR_PI_NETWORK_MODE:-host}") || contents.contains("network_mode: host"),
        "network_mode host fallback is present",
        "network_mode host fallback was not found in compose.yaml",
    ));

    checks.push(check_bool(
        Status::Warn,
        "Container user",
        contents.contains("user: \"0:0\"") || contents.contains("user: '0:0'") || contents.contains("user: 0:0"),
        "container-root runtime is preserved",
        "container-root runtime setting was not found in compose.yaml",
    ));

    checks.push(check_bool(
        Status::Warn,
        "SSH mount model",
        contents.contains(".vegasroom/ssh") && contents.contains("target: /home/agent/.ssh"),
        "SSH directory mount is preserved",
        "SSH directory mount was not found in compose.yaml",
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

    let pass = checks.iter().filter(|check| check.status == Status::Pass).count();
    let warn = checks.iter().filter(|check| check.status == Status::Warn).count();
    let fail = checks.iter().filter(|check| check.status == Status::Fail).count();

    println!("\nSummary: {pass} PASS, {warn} WARN, {fail} FAIL");
}
