use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};

use crate::{
    config::{Config, SelectedSshKey},
    harness,
    paths::{display_path, StatePaths},
    ssh::{self, SshRuntime, SshRuntimeMode},
    workspace::{self, ResolvedWorkspace},
};

#[derive(Debug)]
pub struct SshAddCheck {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub struct ContainerDoctorProbe {
    pub pi_config_writable: bool,
    pub pi_sessions_writable: bool,
    pub internet_reachable: bool,
    pub git_identity: Option<GitIdentity>,
}

#[derive(Debug)]
pub struct ContainerSshDoctorProbe {
    pub receives_ssh_auth_sock: bool,
    pub has_ssh_add: bool,
    pub ssh_add: SshAddCheck,
}

pub fn build_pi_image(config: &Config) -> Result<()> {
    let compose_file = config.resolved_compose_file()?;
    let project_dir = compose_project_dir(&compose_file)?;

    let mut command = base_docker(config);
    apply_compose_config_env(&mut command, config);
    let status = command
        .arg("compose")
        .arg("-f")
        .arg(&compose_file)
        .arg("--project-directory")
        .arg(&project_dir)
        .args(["build", harness::PI.service_name])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to start Docker build command")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "Docker Compose build failed with status: {}",
            status
        ))
    }
}

pub fn run_pi(config: &Config, workspace: &ResolvedWorkspace, pi_args: &[String]) -> Result<i32> {
    run_compose(config, workspace, &pi_compose_args(config, pi_args), true)
}

pub fn run_shell(config: &Config, workspace: &ResolvedWorkspace) -> Result<i32> {
    run_compose(
        config,
        workspace,
        &[
            "run".to_owned(),
            "--rm".to_owned(),
            harness::PI.service_name.to_owned(),
            "sh".to_owned(),
        ],
        true,
    )
}

pub fn ensure_pi_image_exists(config: &Config) -> Result<()> {
    if image_exists(config)? {
        Ok(())
    } else {
        Err(anyhow!("image not found: {}", config.harness.pi.image))
    }
}

pub fn image_exists(config: &Config) -> Result<bool> {
    let status = base_docker(config)
        .args(["image", "inspect", config.harness.pi.image.as_str()])
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

pub fn container_doctor_probe(config: &Config) -> Result<ContainerDoctorProbe> {
    let output = compose_shell_output_without_ssh(
        config,
        r#"
set +e

tmp=/home/agent/.pi/agent/.vr-m4-write-test
if echo m4 > "$tmp" 2>/dev/null && rm -f "$tmp"; then
  echo 'VR_CHECK pi_config_writable=pass'
else
  echo 'VR_CHECK pi_config_writable=fail'
fi

tmp=/home/agent/.pi/sessions/.vr-m4-write-test
if echo m4 > "$tmp" 2>/dev/null && rm -f "$tmp"; then
  echo 'VR_CHECK pi_sessions_writable=pass'
else
  echo 'VR_CHECK pi_sessions_writable=fail'
fi

if node -e "fetch('https://pi.dev').then(r => process.exit(r.status > 0 ? 0 : 1)).catch(() => process.exit(1))" >/dev/null 2>/dev/null; then
  echo 'VR_CHECK internet=pass'
else
  echo 'VR_CHECK internet=fail'
fi

printf 'VR_GIT_NAME=%s\n' "${GIT_AUTHOR_NAME:-}"
printf 'VR_GIT_EMAIL=%s\n' "${GIT_AUTHOR_EMAIL:-}"
"#,
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(anyhow!("container doctor probe failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(ContainerDoctorProbe {
        pi_config_writable: check_passed(&stdout, "pi_config_writable"),
        pi_sessions_writable: check_passed(&stdout, "pi_sessions_writable"),
        internet_reachable: check_passed(&stdout, "internet"),
        git_identity: git_identity_from_parts(
            line_value(&stdout, "VR_GIT_NAME=")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            line_value(&stdout, "VR_GIT_EMAIL=")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
            "container environment",
        ),
    })
}

pub fn container_ssh_doctor_probe(config: &Config) -> Result<Option<ContainerSshDoctorProbe>> {
    if !ssh::planned_ssh_available(config) {
        return Ok(None);
    }

    let output = compose_shell_output(
        config,
        r#"
set +e

if test "$SSH_AUTH_SOCK" = '/run/vegasroom-ssh-agent.sock' && test -S "$SSH_AUTH_SOCK"; then
  echo 'VR_CHECK ssh_auth_sock=pass'
else
  echo 'VR_CHECK ssh_auth_sock=fail'
fi

if command -v ssh-add >/dev/null 2>/dev/null; then
  echo 'VR_CHECK ssh_add_binary=pass'
  out=/tmp/vr-ssh-add-out.$$
  err=/tmp/vr-ssh-add-err.$$
  ssh-add -l >"$out" 2>"$err"
  code=$?
else
  echo 'VR_CHECK ssh_add_binary=fail'
  out=/tmp/vr-ssh-add-out.$$
  err=/tmp/vr-ssh-add-err.$$
  : >"$out"
  printf '%s\n' 'ssh-add was not found inside the room' >"$err"
  code=127
fi

echo "VR_SSH_ADD_CODE=$code"
while IFS= read -r line; do
  printf 'VR_SSH_ADD_STDOUT=%s\n' "$line"
done <"$out"
while IFS= read -r line; do
  printf 'VR_SSH_ADD_STDERR=%s\n' "$line"
done <"$err"
rm -f "$out" "$err"
"#,
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(anyhow!("container SSH doctor probe failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(Some(ContainerSshDoctorProbe {
        receives_ssh_auth_sock: check_passed(&stdout, "ssh_auth_sock"),
        has_ssh_add: check_passed(&stdout, "ssh_add_binary"),
        ssh_add: SshAddCheck {
            code: line_value(&stdout, "VR_SSH_ADD_CODE=")
                .and_then(|value| value.trim().parse::<i32>().ok())
                .unwrap_or(1),
            stdout: prefixed_lines(&stdout, "VR_SSH_ADD_STDOUT="),
            stderr: prefixed_lines(&stdout, "VR_SSH_ADD_STDERR="),
        },
    }))
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
        .args(["run", "--rm", harness::PI.service_name, "sh", "-lc", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to start Docker Compose check command")
}

fn check_passed(output: &str, name: &str) -> bool {
    let prefix = format!("VR_CHECK {name}=");
    line_value(output, &prefix)
        .map(|value| value.trim() == "pass")
        .unwrap_or(false)
}

fn line_value<'a>(output: &'a str, prefix: &str) -> Option<&'a str> {
    output.lines().find_map(|line| line.strip_prefix(prefix))
}

fn prefixed_lines(output: &str, prefix: &str) -> String {
    output
        .lines()
        .filter_map(|line| line.strip_prefix(prefix))
        .collect::<Vec<_>>()
        .join("\n")
}

struct ComposeInvocation {
    command: Command,
    _runtime_files: RuntimeFiles,
    _ssh_runtime: SshRuntime,
}

static RUNTIME_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct RuntimeFiles {
    dir: PathBuf,
}

impl RuntimeFiles {
    fn new(state: &StatePaths) -> Result<Self> {
        fs::create_dir_all(&state.cache).with_context(|| {
            format!(
                "failed to create cache directory: {}",
                display_path(&state.cache)
            )
        })?;

        for _ in 0..10 {
            let dir = unique_runtime_dir(&state.cache);
            match fs::create_dir(&dir) {
                Ok(()) => return Ok(Self { dir }),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!(
                            "failed to create per-launch runtime directory: {}",
                            display_path(&dir)
                        )
                    });
                }
            }
        }

        Err(anyhow!(
            "failed to allocate a unique per-launch runtime directory under {}",
            display_path(&state.cache)
        ))
    }

    fn dir(&self) -> &Path {
        &self.dir
    }
}

impl Drop for RuntimeFiles {
    fn drop(&mut self) {
        if self
            .dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("run-"))
            .unwrap_or(false)
        {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

fn unique_runtime_dir(cache: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = RUNTIME_COUNTER.fetch_add(1, Ordering::Relaxed);
    cache.join(format!("run-{}-{nanos}-{counter}", std::process::id()))
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
        prepare_git_identity_override(config, runtime_files.dir(), warn_about_ssh)?
    {
        command.arg("-f").arg(git_override_path);
    }

    if let Some(read_only_rootfs_override_path) =
        prepare_read_only_rootfs_override(config, runtime_files.dir())?
    {
        command.arg("-f").arg(read_only_rootfs_override_path);
    }

    Ok(ComposeInvocation {
        command,
        _runtime_files: runtime_files,
        _ssh_runtime: ssh_runtime,
    })
}

#[derive(Debug, Clone)]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
    pub source: String,
}

pub fn effective_git_identity(config: &Config) -> Option<GitIdentity> {
    if let Some(identity) = configured_git_identity(config) {
        return Some(identity);
    }

    if let Some(identity) = single_selected_key_git_identity(&config.ssh.selected_keys) {
        return Some(identity);
    }

    if config.git.inherit_host {
        return host_git_identity();
    }

    None
}

fn prepare_read_only_rootfs_override(
    config: &Config,
    runtime_dir: &Path,
) -> Result<Option<PathBuf>> {
    if !config.harness.pi.read_only_rootfs {
        return Ok(None);
    }

    fs::create_dir_all(runtime_dir).with_context(|| {
        format!(
            "failed to create per-launch runtime directory: {}",
            display_path(runtime_dir)
        )
    })?;

    let override_path = runtime_dir.join("read-only-rootfs.compose.yaml");
    let contents = r#"services:
  pi:
    read_only: true
    tmpfs:
      - /tmp
      - /run
      - /var/tmp
"#;

    fs::write(&override_path, contents).with_context(|| {
        format!(
            "failed to write read-only rootfs Compose override: {}",
            display_path(&override_path)
        )
    })?;

    Ok(Some(override_path))
}

fn prepare_git_identity_override(
    config: &Config,
    runtime_dir: &Path,
    warn: bool,
) -> Result<Option<PathBuf>> {
    let Some(identity) = effective_git_identity(config) else {
        if warn {
            eprintln!(
                "WARN: no Git identity configured or inherited; commits may fall back to the container user. Set git.user_name/git.user_email in ~/.vegasroom/config.yaml."
            );
        }
        return Ok(None);
    };

    fs::create_dir_all(runtime_dir).with_context(|| {
        format!(
            "failed to create per-launch runtime directory: {}",
            display_path(runtime_dir)
        )
    })?;

    let gitconfig_path = runtime_dir.join("gitconfig");
    fs::write(&gitconfig_path, gitconfig_contents(&identity)).with_context(|| {
        format!(
            "failed to write Git identity config: {}",
            display_path(&gitconfig_path)
        )
    })?;

    let override_path = runtime_dir.join("git-identity.compose.yaml");
    let contents = format!(
        r#"services:
  pi:
    environment:
      GIT_CONFIG_GLOBAL: /run/vegasroom-gitconfig
      GIT_AUTHOR_NAME: "{name}"
      GIT_AUTHOR_EMAIL: "{email}"
      GIT_COMMITTER_NAME: "{name}"
      GIT_COMMITTER_EMAIL: "{email}"
    volumes:
      - type: bind
        source: "{gitconfig_path}"
        target: /run/vegasroom-gitconfig
        read_only: true
"#,
        name = yaml_double_quoted_str(&identity.name),
        email = yaml_double_quoted_str(&identity.email),
        gitconfig_path = yaml_double_quoted_path(&gitconfig_path),
    );

    fs::write(&override_path, contents).with_context(|| {
        format!(
            "failed to write Git identity Compose override: {}",
            display_path(&override_path)
        )
    })?;

    Ok(Some(override_path))
}

fn configured_git_identity(config: &Config) -> Option<GitIdentity> {
    let name = non_empty_trimmed(config.git.user_name.as_deref())?;
    let email = non_empty_trimmed(config.git.user_email.as_deref())?;

    Some(GitIdentity {
        name: name.to_owned(),
        email: email.to_owned(),
        source: "~/.vegasroom/config.yaml git.user_name/git.user_email".to_owned(),
    })
}

fn single_selected_key_git_identity(keys: &[SelectedSshKey]) -> Option<GitIdentity> {
    let mut identities = keys.iter().filter_map(selected_key_git_identity);
    let identity = identities.next()?;

    if identities.next().is_none() {
        Some(identity)
    } else {
        None
    }
}

fn selected_key_git_identity(key: &SelectedSshKey) -> Option<GitIdentity> {
    let name = non_empty_trimmed(key.git_user_name.as_deref())?;
    let email = non_empty_trimmed(key.git_user_email.as_deref())?;

    Some(GitIdentity {
        name: name.to_owned(),
        email: email.to_owned(),
        source: format!("selected SSH key metadata: {}", key.path),
    })
}

fn host_git_identity() -> Option<GitIdentity> {
    let name = host_git_config_value("user.name")?;
    let email = host_git_config_value("user.email")?;

    Some(GitIdentity {
        name,
        email,
        source: "host git config --global".to_owned(),
    })
}

fn host_git_config_value(key: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--global", "--get", key])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn gitconfig_contents(identity: &GitIdentity) -> String {
    format!(
        "[user]\n\tname = {}\n\temail = {}\n[safe]\n\tdirectory = *\n[init]\n\tdefaultBranch = main\n",
        git_config_value(&identity.name),
        git_config_value(&identity.email),
    )
}

fn git_config_value(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

fn git_identity_from_parts(
    name: Option<String>,
    email: Option<String>,
    source: impl Into<String>,
) -> Option<GitIdentity> {
    match (name, email) {
        (Some(name), Some(email)) => Some(GitIdentity {
            name,
            email,
            source: source.into(),
        }),
        _ => None,
    }
}

fn non_empty_trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn yaml_double_quoted_path(value: &Path) -> String {
    yaml_double_quoted_str(&value.display().to_string())
}

fn yaml_double_quoted_str(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\r', '\n'], " ")
}

fn compose_project_dir(compose_file: &std::path::Path) -> Result<std::path::PathBuf> {
    compose_file
        .parent()
        .map(std::path::Path::to_path_buf)
        .context("Compose file has no parent directory")
}

fn pi_compose_args(config: &Config, pi_args: &[String]) -> Vec<String> {
    let mut compose_args = vec![
        "run".to_owned(),
        "--rm".to_owned(),
        harness::PI.service_name.to_owned(),
        config.harness.pi.command.clone(),
    ];
    compose_args.extend(pi_args.iter().cloned());
    compose_args
}

fn apply_compose_config_env(command: &mut Command, config: &Config) {
    command
        .env("VR_PI_IMAGE", &config.harness.pi.image)
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

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn selected_key(path: &str, name: Option<&str>, email: Option<&str>) -> SelectedSshKey {
        SelectedSshKey {
            path: path.to_owned(),
            fingerprint: Some(format!("SHA256:{path}")),
            comment: None,
            key_type: Some("ED25519".to_owned()),
            git_user_name: name.map(str::to_owned),
            git_user_email: email.map(str::to_owned),
        }
    }

    fn test_state_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("vegasroom-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn pi_compose_args_always_uses_configured_command() {
        let mut config = Config::default();
        config.harness.pi.command = "custom-pi".to_owned();

        assert_eq!(
            pi_compose_args(&config, &[]),
            strings(&["run", "--rm", "pi", "custom-pi"]),
        );
        assert_eq!(
            pi_compose_args(&config, &["--session".to_owned(), "abc".to_owned()]),
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
    fn doctor_probe_parser_reads_structured_lines() {
        let output = "\
noise
VR_CHECK pi_config_writable=pass
VR_CHECK internet=fail
VR_SSH_ADD_STDOUT=one
VR_SSH_ADD_STDOUT=two
VR_SSH_ADD_CODE=1
";

        assert!(check_passed(output, "pi_config_writable"));
        assert!(!check_passed(output, "internet"));
        assert_eq!(line_value(output, "VR_SSH_ADD_CODE="), Some("1"));
        assert_eq!(prefixed_lines(output, "VR_SSH_ADD_STDOUT="), "one\ntwo");
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

    #[test]
    fn read_only_rootfs_override_is_only_written_when_enabled() {
        let root = test_state_root("read-only-rootfs");
        fs::create_dir_all(&root).unwrap();
        let mut config = Config::default();

        assert!(prepare_read_only_rootfs_override(&config, &root)
            .unwrap()
            .is_none());

        config.harness.pi.read_only_rootfs = true;
        let override_path = prepare_read_only_rootfs_override(&config, &root)
            .unwrap()
            .unwrap();
        let contents = fs::read_to_string(&override_path).unwrap();

        assert!(contents.contains("read_only: true"));
        assert!(contents.contains("- /tmp"));
        assert!(contents.contains("- /run"));
        assert!(contents.contains("- /var/tmp"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn git_identity_requires_name_and_email() {
        let complete = git_identity_from_parts(
            Some("Agent User".to_owned()),
            Some("agent@example.com".to_owned()),
            "test",
        );
        let missing_email = git_identity_from_parts(Some("Agent User".to_owned()), None, "test");
        let missing_name =
            git_identity_from_parts(None, Some("agent@example.com".to_owned()), "test");

        assert!(complete.is_some());
        assert!(missing_email.is_none());
        assert!(missing_name.is_none());
    }

    #[test]
    fn configured_git_identity_takes_precedence() {
        let mut config = Config::default();
        config.git.user_name = Some("Configured User".to_owned());
        config.git.user_email = Some("configured@example.com".to_owned());
        config.ssh.selected_keys.push(selected_key(
            "~/.ssh/id_ed25519",
            Some("Key User"),
            Some("key@example.com"),
        ));

        let identity = effective_git_identity(&config).unwrap();

        assert_eq!(identity.name, "Configured User");
        assert_eq!(identity.email, "configured@example.com");
        assert!(identity.source.contains("config.yaml"));
    }

    #[test]
    fn single_selected_key_git_identity_is_used() {
        let mut config = Config::default();
        config.git.inherit_host = false;
        config.ssh.selected_keys.push(selected_key(
            "~/.ssh/id_ed25519",
            Some("Key User"),
            Some("key@example.com"),
        ));

        let identity = effective_git_identity(&config).unwrap();

        assert_eq!(identity.name, "Key User");
        assert_eq!(identity.email, "key@example.com");
        assert!(identity.source.contains("selected SSH key metadata"));
    }

    #[test]
    fn multiple_selected_key_git_identities_are_ambiguous() {
        let mut config = Config::default();
        config.git.inherit_host = false;
        config.ssh.selected_keys.push(selected_key(
            "~/.ssh/id_one",
            Some("One"),
            Some("one@example.com"),
        ));
        config.ssh.selected_keys.push(selected_key(
            "~/.ssh/id_two",
            Some("Two"),
            Some("two@example.com"),
        ));

        assert!(effective_git_identity(&config).is_none());
    }

    #[test]
    fn disabled_host_inheritance_prevents_host_fallback() {
        let mut config = Config::default();
        config.git.inherit_host = false;

        assert!(effective_git_identity(&config).is_none());
    }

    #[test]
    fn gitconfig_sanitizes_newlines() {
        let identity = GitIdentity {
            name: "Agent\nUser".to_owned(),
            email: "agent\r@example.com".to_owned(),
            source: "test".to_owned(),
        };
        let contents = gitconfig_contents(&identity);

        assert!(contents.contains("name = Agent User"));
        assert!(contents.contains("email = agent @example.com"));
        assert!(!contents.contains("Agent\nUser"));
    }

    #[test]
    fn yaml_double_quoted_values_are_escaped() {
        let escaped = yaml_double_quoted_str("Agent \\\"User\"\n");

        assert_eq!(escaped, r#"Agent \\\"User\" "#);
    }
}
