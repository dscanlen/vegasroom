use std::process::{Command, Output, Stdio};

use anyhow::{anyhow, Context, Result};

mod compose_gen;
mod doctor_probe;
pub(crate) mod environment;
mod git_identity;
mod images;
mod overrides;
mod runtime_files;

pub use doctor_probe::{
    container_doctor_probe, container_ssh_doctor_probe, path_contains_entry, ContainerDoctorProbe,
    SshAddCheck,
};
pub use git_identity::{effective_git_identity, GitIdentity};
pub use images::{
    base_status, harness_on_current_base, removal_command, unused_images, BaseStatus,
};
use runtime_files::RuntimeFiles;

use crate::{
    atomic_write,
    config::Config,
    harness,
    paths::{display_path, StatePaths},
    ssh::{self, SshRuntime, SshRuntimeMode},
    workspace::{self, ResolvedWorkspace},
};

pub fn write_runtime_compose_yaml(config: &Config, state: &StatePaths) -> Result<()> {
    let content = compose_gen::generate(harness::HARNESSES, config, state);
    atomic_write::write_file(&state.runtime_compose_yaml, content).with_context(|| {
        format!(
            "failed to write managed Compose file: {}",
            display_path(&state.runtime_compose_yaml)
        )
    })
}

/// Write the generated shared base Dockerfile (foundation + environment profile).
pub fn write_base_dockerfile(config: &Config, state: &StatePaths) -> Result<()> {
    let path = state.runtime_base_dockerfile();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create base runtime directory: {}",
                display_path(parent)
            )
        })?;
    }
    atomic_write::write_file(&path, environment::base_dockerfile_contents(config))
        .with_context(|| format!("failed to write base Dockerfile: {}", display_path(&path)))
}

/// Build the shared base image (foundation + environment profile) every harness builds on.
///
/// Uses the default harness's `build_network`, since the base is shared.
pub fn build_base_image(config: &Config, state: &StatePaths) -> Result<()> {
    let image = &config.environment.base_image;
    let build_network = &config.harness.instance(harness::HARNESSES[0]).build_network;
    println!("Building shared base image: {image}");
    for line in environment::profile_summary(config) {
        println!("  environment profile: {line}");
    }
    // Written at build time so the build context matches what the image was built from.
    write_base_dockerfile(config, state)?;

    let status = base_docker(config)
        .arg("build")
        .arg("--network")
        .arg(build_network)
        .arg("-f")
        .arg(state.runtime_base_dockerfile())
        .arg("-t")
        .arg(image)
        .arg("--label")
        .arg(format!(
            "{}={}",
            environment::BASE_FINGERPRINT_LABEL,
            environment::base_fingerprint(config)
        ))
        .arg(state.runtime_base_dir())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to start Docker base image build command")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "Docker base image build failed with status: {status}"
        ))
    }
}

/// Build a harness image on the shared base (the base must be built first).
pub fn build_image(config: &Config, descriptor: &harness::HarnessDescriptor) -> Result<()> {
    build_harness_image(config, descriptor)
}

pub fn run_harness(
    descriptor: &harness::HarnessDescriptor,
    config: &Config,
    workspace: &ResolvedWorkspace,
    harness_args: &[String],
    state: &StatePaths,
) -> Result<i32> {
    run_harness_command(
        config,
        descriptor,
        workspace,
        harness_command(config, descriptor),
        harness_args,
        state,
    )
}

pub fn run_shell(
    config: &Config,
    workspace: &ResolvedWorkspace,
    command: &[String],
    state: &StatePaths,
) -> Result<i32> {
    let ssh_mode = if command.is_empty() {
        SshRuntimeMode::Interactive
    } else {
        SshRuntimeMode::NonInteractive
    };
    run_compose(
        config,
        &harness::PI,
        workspace,
        &shell_compose_args(command),
        ssh_mode,
        state,
    )
}

fn shell_compose_args(command: &[String]) -> Vec<String> {
    match command.split_first() {
        None => harness_compose_args(&harness::PI, "sh", &[]),
        Some((program, args)) => {
            let mut compose_args = harness_compose_args(&harness::PI, program, args);
            // Explicit commands are suitable for scripts, with no pseudo-TTY.
            compose_args.insert(2, "-T".to_owned());
            compose_args
        }
    }
}

pub fn ensure_harness_image_exists(
    descriptor: &harness::HarnessDescriptor,
    config: &Config,
) -> Result<()> {
    ensure_harness_image(descriptor, config)
}

/// Whether the harness image a launch of `descriptor` would run exists.
pub fn image_exists(config: &Config, descriptor: &harness::HarnessDescriptor) -> Result<bool> {
    harness_image_exists(config, descriptor)
}

/// The image a launch of `descriptor` runs: the configured harness image.
pub fn runtime_image(config: &Config, descriptor: &harness::HarnessDescriptor) -> String {
    harness_image(config, descriptor).to_owned()
}

fn build_harness_image(config: &Config, descriptor: &harness::HarnessDescriptor) -> Result<()> {
    let compose_file = config.resolved_compose_file()?;
    let project_dir = compose_project_dir(&compose_file)?;

    let mut command = base_docker(config);
    apply_compose_config_env(&mut command, config, descriptor);
    command.env(
        descriptor.compose_var("IMAGE"),
        config.harness_runtime_config(descriptor).image,
    );
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
    state: &StatePaths,
) -> Result<i32> {
    run_compose(
        config,
        descriptor,
        workspace,
        &harness_compose_args(descriptor, command, args),
        SshRuntimeMode::Interactive,
        state,
    )
}

fn ensure_harness_image(descriptor: &harness::HarnessDescriptor, config: &Config) -> Result<()> {
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
    descriptor: &harness::HarnessDescriptor,
    workspace: &ResolvedWorkspace,
    compose_args: &[String],
    ssh_mode: SshRuntimeMode,
    state: &StatePaths,
) -> Result<i32> {
    let mut invocation = compose_base(
        config,
        descriptor,
        Some(workspace),
        true,
        true,
        ssh_mode,
        state,
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
    compose_shell_output_with_ssh(config, &harness::PI, script, true)
}

fn compose_shell_output_without_ssh(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
    script: &str,
) -> Result<Output> {
    compose_shell_output_with_ssh(config, descriptor, script, false)
}

fn compose_shell_output_with_ssh(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
    script: &str,
    include_ssh_agent: bool,
) -> Result<Output> {
    let state = StatePaths::default()?;
    let workspace = workspace::default_workspace_for_compose_with_state(config, &state)?;
    let mut invocation = compose_base(
        config,
        descriptor,
        Some(&workspace),
        include_ssh_agent,
        false,
        SshRuntimeMode::NonInteractive,
        &state,
    )?;
    invocation
        .command
        .args(["run", "--rm", descriptor.service_name, "sh", "-c", script])
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
    descriptor: &harness::HarnessDescriptor,
    workspace: Option<&ResolvedWorkspace>,
    include_ssh_agent: bool,
    warn_about_ssh: bool,
    ssh_mode: SshRuntimeMode,
    state: &StatePaths,
) -> Result<ComposeInvocation> {
    let compose_file = config.resolved_compose_file_from_state(state)?;
    let project_dir = compose_project_dir(&compose_file)?;
    let runtime_files = RuntimeFiles::new(state)?;

    let mut command = base_docker(config);
    apply_compose_config_env(&mut command, config, descriptor);
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
        let runtime = ssh::prepare_agent_override(
            config,
            descriptor.service_name,
            runtime_files.dir(),
            warn_about_ssh,
            ssh_mode,
        )?;
        if let Some(override_path) = runtime.override_path() {
            command.arg("-f").arg(override_path);
        }
        runtime
    } else {
        SshRuntime::empty()
    };

    if let Some(git_override_path) = git_identity::prepare_override(
        config,
        descriptor.service_name,
        runtime_files.dir(),
        warn_about_ssh,
    )? {
        command.arg("-f").arg(git_override_path);
    }

    if let Some(read_only_rootfs_override_path) =
        overrides::prepare_read_only_rootfs(config, descriptor, runtime_files.dir())?
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
    config.harness_runtime_config(descriptor).image
}

fn harness_command<'a>(config: &'a Config, descriptor: &harness::HarnessDescriptor) -> &'a str {
    config.harness_runtime_config(descriptor).command
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

/// Set Compose interpolation variables for every registered harness service (the
/// managed compose file defines all of them), plus per-launch values for the
/// service actually being run.
fn apply_compose_config_env(
    command: &mut Command,
    config: &Config,
    launched: &harness::HarnessDescriptor,
) {
    command.env("VR_BASE_IMAGE", &config.environment.base_image);
    for descriptor in harness::HARNESSES {
        let instance = config.harness.instance(descriptor);
        command
            .env(
                descriptor.compose_var("IMAGE"),
                harness_image(config, descriptor),
            )
            .env(descriptor.compose_var("NETWORK_MODE"), &instance.network)
            .env(
                descriptor.compose_var("BUILD_NETWORK"),
                &instance.build_network,
            );
    }
    command.env(
        "VR_WORKSPACE_READ_ONLY",
        config
            .harness
            .instance(launched)
            .read_only_workspace
            .to_string(),
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
    use crate::test_util::unique_temp_dir;
    use std::fs;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn shell_commands_disable_tty_and_preserve_argv() {
        assert_eq!(
            shell_compose_args(&strings(&["sh", "-eu", "-c", "echo hello world"])),
            strings(&[
                "run",
                "--rm",
                "-T",
                "pi",
                "sh",
                "-eu",
                "-c",
                "echo hello world"
            ])
        );
        assert_eq!(
            shell_compose_args(&[]),
            strings(&["run", "--rm", "pi", "sh"])
        );
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

        apply_compose_config_env(&mut command, &config, &harness::PI);

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
        assert!(envs.contains(&(
            "VR_BASE_IMAGE".to_owned(),
            Some("vegasroom/base:latest".to_owned()),
        )));
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
        let root = unique_temp_dir("runtime-files");
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
