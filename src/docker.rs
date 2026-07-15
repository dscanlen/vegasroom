use std::process::{Command, Output, Stdio};

use anyhow::{anyhow, Context, Result};

mod doctor_probe;
mod environment;
mod git_identity;
mod overrides;
mod runtime_files;

pub use doctor_probe::{
    container_doctor_probe, container_ssh_doctor_probe, ContainerDoctorProbe, SshAddCheck,
};
pub use git_identity::{effective_git_identity, GitIdentity};
use runtime_files::RuntimeFiles;

use crate::{
    config::Config,
    harness,
    paths::StatePaths,
    ssh::{self, SshRuntime, SshRuntimeMode},
    workspace::{self, ResolvedWorkspace},
};

pub fn build_pi_image(config: &Config) -> Result<()> {
    build_harness_image(config, &harness::PI)?;
    environment::build_image(config, &harness::PI)
}

pub fn run_pi(config: &Config, workspace: &ResolvedWorkspace, pi_args: &[String]) -> Result<i32> {
    run_harness_command(
        config,
        &harness::PI,
        workspace,
        harness_command(config, &harness::PI),
        pi_args,
    )
}

pub fn run_shell(config: &Config, workspace: &ResolvedWorkspace) -> Result<i32> {
    run_harness_command(config, &harness::PI, workspace, "sh", &[])
}

pub fn ensure_pi_image_exists(config: &Config) -> Result<()> {
    ensure_harness_image_exists(config, &harness::PI)
}

pub fn image_exists(config: &Config) -> Result<bool> {
    if environment::has_customization(config) {
        environment::image_exists(config, &harness::PI)
    } else {
        harness_image_exists(config, &harness::PI)
    }
}

pub fn environment_image_stale(config: &Config) -> Result<bool> {
    environment::image_stale(config, &harness::PI)
}

pub fn pi_runtime_image(config: &Config) -> String {
    environment::runtime_image(config, &harness::PI)
}

pub fn environment_apt_packages(config: &Config) -> Vec<String> {
    environment::packages(config)
}

pub fn environment_rust_enabled(config: &Config) -> bool {
    environment::rust_enabled(config)
}

pub fn environment_rust_toolchain(config: &Config) -> String {
    environment::rust_toolchain(config)
}

pub fn environment_rust_components(config: &Config) -> Vec<String> {
    environment::rust_components(config)
}

pub fn environment_python_enabled(config: &Config) -> bool {
    environment::python_enabled(config)
}

pub fn environment_go_enabled(config: &Config) -> bool {
    environment::go_enabled(config)
}

pub fn environment_typescript_enabled(config: &Config) -> bool {
    environment::typescript_enabled(config)
}

pub fn environment_typescript_packages(config: &Config) -> Vec<String> {
    environment::typescript_packages(config)
}

fn build_harness_image(config: &Config, descriptor: &harness::HarnessDescriptor) -> Result<()> {
    let compose_file = config.resolved_compose_file()?;
    let project_dir = compose_project_dir(&compose_file)?;

    let mut command = base_docker(config);
    apply_compose_config_env(&mut command, config);
    if descriptor.id == harness::PI.id {
        command.env("VR_PI_IMAGE", &config.harness.pi.image);
    }
    let status = command
        .arg("compose")
        .arg("-f")
        .arg(&compose_file)
        .arg("--project-directory")
        .arg(&project_dir)
        .args(["build", descriptor.service_name])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to start Docker build command")?;

    if status.success() {
        tag_standard_harness_images(config, descriptor)
    } else {
        Err(anyhow!(
            "Docker Compose build failed with status: {}",
            status
        ))
    }
}

fn tag_standard_harness_images(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
) -> Result<()> {
    let image = harness_image(config, descriptor);
    for tag in standard_harness_image_tags(image, descriptor) {
        let status = base_docker(config)
            .args(["image", "tag", image, tag])
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("failed to start Docker image tag command for {tag}"))?;

        if !status.success() {
            return Err(anyhow!(
                "Docker image tag failed for {image} -> {tag} with status: {status}"
            ));
        }
    }

    Ok(())
}

fn standard_harness_image_tags<'a>(
    image: &'a str,
    descriptor: &'a harness::HarnessDescriptor,
) -> Vec<&'a str> {
    if image == descriptor.default_image {
        vec![descriptor.versioned_image]
    } else if image == descriptor.versioned_image {
        vec![descriptor.default_image]
    } else {
        Vec::new()
    }
}

fn run_harness_command(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
    workspace: &ResolvedWorkspace,
    command: &str,
    args: &[String],
) -> Result<i32> {
    run_compose(
        config,
        workspace,
        &harness_compose_args(descriptor, command, args),
        true,
    )
}

fn ensure_harness_image_exists(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
) -> Result<()> {
    if descriptor.id == harness::PI.id && environment::has_customization(config) {
        return environment::ensure_image(config, descriptor);
    }

    if harness_image_exists(config, descriptor)? {
        Ok(())
    } else {
        Err(anyhow!(
            "image not found: {}",
            harness_image(config, descriptor)
        ))
    }
}

fn harness_image_exists(config: &Config, descriptor: &harness::HarnessDescriptor) -> Result<bool> {
    let status = base_docker(config)
        .args(["image", "inspect", harness_image(config, descriptor)])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to inspect Docker image")?;

    Ok(status.success())
}

pub fn docker_command_available() -> bool {
    Command::new("docker")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn compose_available() -> bool {
    Command::new("docker")
        .args(["compose", "version"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn context_exists(config: &Config) -> bool {
    Command::new("docker")
        .args(["context", "inspect", config.docker.context.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn context_usable(config: &Config) -> bool {
    base_docker(config)
        .arg("info")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn can_run_trivial_container(config: &Config) -> bool {
    base_docker(config)
        .args(["run", "--rm", "--network"])
        .arg(&config.harness.pi.network)
        .arg("hello-world")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_compose(
    config: &Config,
    workspace: &ResolvedWorkspace,
    compose_args: &[String],
    warn_about_ssh: bool,
) -> Result<i32> {
    let mut invocation = compose_base(
        config,
        Some(workspace),
        true,
        warn_about_ssh,
        SshRuntimeMode::Interactive,
    )?;
    let status = invocation
        .command
        .args(compose_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to start Docker Compose command")?;

    Ok(status.code().unwrap_or(1))
}

fn compose_shell_output(config: &Config, script: &str) -> Result<Output> {
    compose_shell_output_with_ssh(config, script, true)
}

fn compose_shell_output_without_ssh(config: &Config, script: &str) -> Result<Output> {
    compose_shell_output_with_ssh(config, script, false)
}

fn compose_shell_output_with_ssh(
    config: &Config,
    script: &str,
    include_ssh_agent: bool,
) -> Result<Output> {
    let workspace = workspace::default_workspace_for_compose(config)?;
    let mut invocation = compose_base(
        config,
        Some(&workspace),
        include_ssh_agent,
        false,
        SshRuntimeMode::NonInteractive,
    )?;
    invocation
        .command
        .args(["run", "--rm", harness::PI.service_name, "sh", "-c", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to start Docker Compose check command")
}

struct ComposeInvocation {
    command: Command,
    _runtime_files: RuntimeFiles,
    _ssh_runtime: SshRuntime,
}

fn compose_base(
    config: &Config,
    workspace: Option<&ResolvedWorkspace>,
    include_ssh_agent: bool,
    warn_about_ssh: bool,
    ssh_mode: SshRuntimeMode,
) -> Result<ComposeInvocation> {
    let compose_file = config.resolved_compose_file()?;
    let project_dir = compose_project_dir(&compose_file)?;
    let state = StatePaths::default()?;
    let runtime_files = RuntimeFiles::new(&state)?;

    let mut command = base_docker(config);
    apply_compose_config_env(&mut command, config);
    command
        .arg("compose")
        .arg("-f")
        .arg(compose_file)
        .arg("--project-directory")
        .arg(project_dir);

    if let Some(workspace) = workspace {
        command.env("VR_WORKSPACE", workspace.path());
    }

    let ssh_runtime = if include_ssh_agent {
        let runtime =
            ssh::prepare_agent_override(config, runtime_files.dir(), warn_about_ssh, ssh_mode)?;
        if let Some(override_path) = runtime.override_path() {
            command.arg("-f").arg(override_path);
        }
        runtime
    } else {
        SshRuntime::empty()
    };

    if let Some(git_override_path) =
        git_identity::prepare_override(config, runtime_files.dir(), warn_about_ssh)?
    {
        command.arg("-f").arg(git_override_path);
    }

    if let Some(read_only_rootfs_override_path) =
        overrides::prepare_read_only_rootfs(config, runtime_files.dir())?
    {
        command.arg("-f").arg(read_only_rootfs_override_path);
    }

    Ok(ComposeInvocation {
        command,
        _runtime_files: runtime_files,
        _ssh_runtime: ssh_runtime,
    })
}

fn compose_project_dir(compose_file: &std::path::Path) -> Result<std::path::PathBuf> {
    compose_file
        .parent()
        .map(std::path::Path::to_path_buf)
        .context("Compose file has no parent directory")
}

fn harness_image<'a>(config: &'a Config, descriptor: &harness::HarnessDescriptor) -> &'a str {
    if descriptor.id == harness::PI.id {
        config.harness.pi.image.as_str()
    } else {
        descriptor.default_image
    }
}

fn harness_command<'a>(config: &'a Config, descriptor: &harness::HarnessDescriptor) -> &'a str {
    if descriptor.id == harness::PI.id {
        config.harness.pi.command.as_str()
    } else {
        descriptor.default_command
    }
}

fn harness_compose_args(
    descriptor: &harness::HarnessDescriptor,
    command: &str,
    args: &[String],
) -> Vec<String> {
    let mut compose_args = vec![
        "run".to_owned(),
        "--rm".to_owned(),
        descriptor.service_name.to_owned(),
        command.to_owned(),
    ];
    compose_args.extend(args.iter().cloned());
    compose_args
}

fn apply_compose_config_env(command: &mut Command, config: &Config) {
    command
        .env(
            "VR_PI_IMAGE",
            environment::runtime_image(config, &harness::PI),
        )
        .env("VR_PI_NETWORK_MODE", &config.harness.pi.network)
        .env("VR_PI_BUILD_NETWORK", &config.harness.pi.build_network)
        .env(
            "VR_WORKSPACE_READ_ONLY",
            config.harness.pi.read_only_workspace.to_string(),
        );
}

fn base_docker(config: &Config) -> Command {
    let mut command = Command::new("docker");
    command.args(["--context", config.docker.context.as_str()]);
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn test_state_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("vegasroom-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn harness_compose_args_uses_descriptor_service_and_configured_command() {
        let mut config = Config::default();
        config.harness.pi.command = "custom-pi".to_owned();
        let command = harness_command(&config, &harness::PI);

        assert_eq!(
            harness_compose_args(&harness::PI, command, &[]),
            strings(&["run", "--rm", "pi", "custom-pi"]),
        );
        assert_eq!(
            harness_compose_args(
                &harness::PI,
                command,
                &["--session".to_owned(), "abc".to_owned()]
            ),
            strings(&["run", "--rm", "pi", "custom-pi", "--session", "abc"]),
        );
    }

    #[test]
    fn compose_config_env_uses_configured_image_network_and_workspace_mode() {
        let mut config = Config::default();
        config.harness.pi.image = "example/pi:test".to_owned();
        config.harness.pi.network = "bridge".to_owned();
        config.harness.pi.build_network = "host".to_owned();
        config.harness.pi.read_only_workspace = true;
        let mut command = Command::new("docker");

        apply_compose_config_env(&mut command, &config);

        let envs = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().to_string(),
                    value.map(|value| value.to_string_lossy().to_string()),
                )
            })
            .collect::<Vec<_>>();
        assert!(envs.contains(&("VR_PI_IMAGE".to_owned(), Some("example/pi:test".to_owned()),)));
        assert!(envs.contains(&("VR_PI_NETWORK_MODE".to_owned(), Some("bridge".to_owned()),)));
        assert!(envs.contains(&("VR_PI_BUILD_NETWORK".to_owned(), Some("host".to_owned()),)));
        assert!(envs.contains(&("VR_WORKSPACE_READ_ONLY".to_owned(), Some("true".to_owned()),)));
    }

    #[test]
    fn standard_harness_tags_link_latest_and_versioned_images() {
        assert_eq!(
            standard_harness_image_tags(harness::PI.default_image, &harness::PI),
            vec![harness::PI.versioned_image]
        );
        assert_eq!(
            standard_harness_image_tags(harness::PI.versioned_image, &harness::PI),
            vec![harness::PI.default_image]
        );
        assert!(standard_harness_image_tags("example/pi:custom", &harness::PI).is_empty());
    }

    #[test]
    fn runtime_files_use_unique_dirs_and_cleanup_on_drop() {
        let root = test_state_root("runtime-files");
        let state = StatePaths::from_root(root.clone());
        let first_dir;
        let second_dir;

        {
            let first = RuntimeFiles::new(&state).unwrap();
            let second = RuntimeFiles::new(&state).unwrap();
            first_dir = first.dir().to_path_buf();
            second_dir = second.dir().to_path_buf();

            assert_ne!(first_dir, second_dir);
            assert!(first_dir.is_dir());
            assert!(second_dir.is_dir());
            assert_eq!(first_dir.parent(), Some(state.cache.as_path()));
            assert_eq!(second_dir.parent(), Some(state.cache.as_path()));
        }

        assert!(!first_dir.exists());
        assert!(!second_dir.exists());
        let _ = fs::remove_dir_all(root);
    }
}
