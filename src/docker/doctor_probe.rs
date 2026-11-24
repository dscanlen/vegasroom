use anyhow::{anyhow, Result};

use crate::{config::Config, harness, ssh};

use super::{compose_shell_output, compose_shell_output_without_ssh, git_identity, GitIdentity};

#[derive(Debug)]
pub struct SshAddCheck {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub struct ContainerDoctorProbe {
    pub state_dirs: Vec<StateDirProbe>,
    pub container_path: String,
    pub command: String,
    pub command_path: Option<String>,
    pub command_version: Option<String>,
    pub python_available: bool,
    pub python_version: Option<String>,
    pub go_available: bool,
    pub go_version: Option<String>,
    pub typescript_available: bool,
    pub tsc_version: Option<String>,
    pub connectivity_url: &'static str,
    pub internet_reachable: bool,
    pub git_identity: Option<GitIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDirProbe {
    pub name: &'static str,
    pub container_path: &'static str,
    pub writable: bool,
}

#[derive(Debug)]
pub struct ContainerSshDoctorProbe {
    pub receives_ssh_auth_sock: bool,
    pub has_ssh_add: bool,
    pub ssh_add: SshAddCheck,
}

/// Run the doctor probe script inside `descriptor`'s room service.
pub fn container_doctor_probe(
    config: &Config,
    descriptor: &harness::HarnessDescriptor,
) -> Result<ContainerDoctorProbe> {
    let command = config.harness_runtime_config(descriptor).command;
    let script = probe_script(descriptor, command);
    let output = compose_shell_output_without_ssh(config, descriptor, &script)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(anyhow!("container doctor probe failed: {stderr}"));
    }

    Ok(parse_probe(
        &String::from_utf8_lossy(&output.stdout),
        descriptor,
        command,
    ))
}

/// `command` is config-validated as shell-free (no metacharacters/whitespace).
fn probe_script(descriptor: &harness::HarnessDescriptor, command: &str) -> String {
    let mut script = String::from("\nset +e\n");

    for dir in descriptor.state_dirs {
        script.push_str(&format!(
            r#"
tmp="{path}/.vr-doctor-write-test"
if echo ok > "$tmp" 2>/dev/null && rm -f "$tmp"; then
  echo 'VR_CHECK state_dir:{name}=pass'
else
  echo 'VR_CHECK state_dir:{name}=fail'
fi
"#,
            path = dir.container_path,
            name = dir.name,
        ));
    }

    script.push_str(&format!(
        r#"
printf 'VR_PATH=%s\n' "$PATH"

command_path="$(command -v {command} 2>/dev/null || true)"
printf 'VR_COMMAND_PATH=%s\n' "$command_path"
if test -n "$command_path"; then
  command_version="$(timeout 20 {command} --version </dev/null 2>/dev/null | head -n 1)"
  printf 'VR_COMMAND_VERSION=%s\n' "$command_version"
fi

if command -v python >/dev/null 2>/dev/null && command -v python3 >/dev/null 2>/dev/null && python3 -m pip --version >/dev/null 2>/dev/null && python3 -m venv --help >/dev/null 2>/dev/null; then
  echo 'VR_CHECK python=pass'
else
  echo 'VR_CHECK python=fail'
fi
python_version="$(python3 --version 2>/dev/null || true)"
printf 'VR_PYTHON_VERSION=%s\n' "$python_version"

if command -v go >/dev/null 2>/dev/null && command -v gofmt >/dev/null 2>/dev/null; then
  echo 'VR_CHECK go=pass'
else
  echo 'VR_CHECK go=fail'
fi
go_version="$(go version 2>/dev/null || true)"
printf 'VR_GO_VERSION=%s\n' "$go_version"

if command -v tsc >/dev/null 2>/dev/null; then
  echo 'VR_CHECK typescript=pass'
else
  echo 'VR_CHECK typescript=fail'
fi
tsc_version="$(tsc --version 2>/dev/null || true)"
printf 'VR_TSC_VERSION=%s\n' "$tsc_version"

if node -e "fetch('{url}').then(r => process.exit(r.status > 0 ? 0 : 1)).catch(() => process.exit(1))" >/dev/null 2>/dev/null; then
  echo 'VR_CHECK internet=pass'
else
  echo 'VR_CHECK internet=fail'
fi

printf 'VR_GIT_NAME=%s\n' "${{GIT_AUTHOR_NAME:-}}"
printf 'VR_GIT_EMAIL=%s\n' "${{GIT_AUTHOR_EMAIL:-}}"
"#,
        url = descriptor.connectivity_url,
    ));

    script
}

fn parse_probe(
    stdout: &str,
    descriptor: &harness::HarnessDescriptor,
    command: &str,
) -> ContainerDoctorProbe {
    let non_empty = |prefix: &str| {
        line_value(stdout, prefix)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };

    ContainerDoctorProbe {
        state_dirs: descriptor
            .state_dirs
            .iter()
            .map(|dir| StateDirProbe {
                name: dir.name,
                container_path: dir.container_path,
                writable: check_passed(stdout, &format!("state_dir:{}", dir.name)),
            })
            .collect(),
        container_path: line_value(stdout, "VR_PATH=")
            .map(str::trim)
            .unwrap_or_default()
            .to_owned(),
        command: command.to_owned(),
        command_path: non_empty("VR_COMMAND_PATH="),
        command_version: non_empty("VR_COMMAND_VERSION="),
        python_available: check_passed(stdout, "python"),
        python_version: non_empty("VR_PYTHON_VERSION="),
        go_available: check_passed(stdout, "go"),
        go_version: non_empty("VR_GO_VERSION="),
        typescript_available: check_passed(stdout, "typescript"),
        tsc_version: non_empty("VR_TSC_VERSION="),
        connectivity_url: descriptor.connectivity_url,
        internet_reachable: check_passed(stdout, "internet"),
        git_identity: git_identity::git_identity_from_parts(
            non_empty("VR_GIT_NAME="),
            non_empty("VR_GIT_EMAIL="),
            "container environment",
        ),
    }
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

fn check_passed(output: &str, name: &str) -> bool {
    let prefix = format!("VR_CHECK {name}=");
    line_value(output, &prefix)
        .map(|value| value.trim() == "pass")
        .unwrap_or(false)
}

fn line_value<'a>(output: &'a str, prefix: &str) -> Option<&'a str> {
    output.lines().find_map(|line| line.strip_prefix(prefix))
}

pub fn path_contains_entry(path: &str, entry: &str) -> bool {
    path.split(':').any(|candidate| candidate == entry)
}

fn prefixed_lines(output: &str, prefix: &str) -> String {
    output
        .lines()
        .filter_map(|line| line.strip_prefix(prefix))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_probe_parser_reads_structured_lines() {
        let output = "\
noise
VR_CHECK state_dir:config=pass
VR_CHECK state_dir:sessions=fail
VR_CHECK state_dir:npm-global=pass
VR_PATH=/home/agent/.npm-global/bin:/usr/local/bin:/usr/bin
VR_COMMAND_PATH=/home/agent/.npm-global/bin/pi
VR_COMMAND_VERSION=1.6.0
VR_CHECK python=pass
VR_PYTHON_VERSION=Python 3.11.2
VR_CHECK go=pass
VR_GO_VERSION=go version go1.23.0 linux/amd64
VR_CHECK typescript=pass
VR_TSC_VERSION=Version 5.9.3
VR_CHECK internet=fail
VR_GIT_NAME=Ada
VR_GIT_EMAIL=ada@example.com
VR_SSH_ADD_STDOUT=one
VR_SSH_ADD_STDOUT=two
VR_SSH_ADD_CODE=1
";

        let probe = parse_probe(output, &harness::PI, "pi");
        let writable = |name: &str| {
            probe
                .state_dirs
                .iter()
                .find(|dir| dir.name == name)
                .map(|dir| dir.writable)
        };
        assert_eq!(probe.state_dirs.len(), harness::PI.state_dirs.len());
        assert_eq!(writable("config"), Some(true));
        assert_eq!(writable("sessions"), Some(false));
        assert_eq!(writable("npm-global"), Some(true));
        assert_eq!(writable("skills"), Some(false));
        assert!(path_contains_entry(
            &probe.container_path,
            "/home/agent/.npm-global/bin"
        ));
        assert_eq!(
            probe.command_path.as_deref(),
            Some("/home/agent/.npm-global/bin/pi")
        );
        assert_eq!(probe.command_version.as_deref(), Some("1.6.0"));
        assert!(probe.python_available);
        assert_eq!(probe.python_version.as_deref(), Some("Python 3.11.2"));
        assert!(probe.go_available);
        assert!(probe.typescript_available);
        assert_eq!(probe.tsc_version.as_deref(), Some("Version 5.9.3"));
        assert!(!probe.internet_reachable);
        assert_eq!(probe.connectivity_url, "https://pi.dev");
        assert!(probe.git_identity.is_some());
        assert_eq!(line_value(output, "VR_SSH_ADD_CODE="), Some("1"));
        assert_eq!(prefixed_lines(output, "VR_SSH_ADD_STDOUT="), "one\ntwo");
    }

    #[test]
    fn doctor_probe_parser_treats_missing_command_lines_as_none() {
        let probe = parse_probe("VR_COMMAND_PATH=\n", &harness::CLAUDE, "claude");
        assert_eq!(probe.command, "claude");
        assert!(probe.command_path.is_none());
        assert!(probe.command_version.is_none());
        assert_eq!(probe.connectivity_url, "https://api.anthropic.com");
        assert_eq!(probe.state_dirs.len(), 1);
        assert!(!probe.state_dirs[0].writable);
    }

    #[test]
    fn probe_script_covers_each_state_dir_command_and_url() {
        let script = probe_script(&harness::CLAUDE, "claude");
        assert!(script.contains("tmp=\"/home/agent/.claude/.vr-doctor-write-test\""));
        assert!(script.contains("VR_CHECK state_dir:config=pass"));
        assert!(script.contains("command -v claude"));
        assert!(script.contains("timeout 20 claude --version </dev/null"));
        assert!(script.contains("fetch('https://api.anthropic.com')"));
        assert!(!script.contains(".pi/"));
    }
}
