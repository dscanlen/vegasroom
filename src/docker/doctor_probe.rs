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
    pub pi_config_writable: bool,
    pub pi_sessions_writable: bool,
    pub pi_npm_global_writable: bool,
    pub npm_global_bin_on_path: bool,
    pub container_path: String,
    pub pi_command_path: Option<String>,
    pub python_available: bool,
    pub python_version: Option<String>,
    pub internet_reachable: bool,
    pub git_identity: Option<GitIdentity>,
}

#[derive(Debug)]
pub struct ContainerSshDoctorProbe {
    pub receives_ssh_auth_sock: bool,
    pub has_ssh_add: bool,
    pub ssh_add: SshAddCheck,
}

pub fn container_doctor_probe(config: &Config) -> Result<ContainerDoctorProbe> {
    let pi_config_path = harness::PI.required_state_dir_container_path(harness::PI_CONFIG_DIR);
    let pi_sessions_path = harness::PI.required_state_dir_container_path(harness::PI_SESSIONS_DIR);
    let pi_npm_global_path =
        harness::PI.required_state_dir_container_path(harness::PI_NPM_GLOBAL_DIR);
    let script = format!(
        r#"
set +e

tmp="{pi_config_path}/.vr-m4-write-test"
if echo m4 > "$tmp" 2>/dev/null && rm -f "$tmp"; then
  echo 'VR_CHECK pi_config_writable=pass'
else
  echo 'VR_CHECK pi_config_writable=fail'
fi

tmp="{pi_sessions_path}/.vr-m4-write-test"
if echo m4 > "$tmp" 2>/dev/null && rm -f "$tmp"; then
  echo 'VR_CHECK pi_sessions_writable=pass'
else
  echo 'VR_CHECK pi_sessions_writable=fail'
fi

tmp="{pi_npm_global_path}/.vr-m4-write-test"
if echo m4 > "$tmp" 2>/dev/null && rm -f "$tmp"; then
  echo 'VR_CHECK pi_npm_global_writable=pass'
else
  echo 'VR_CHECK pi_npm_global_writable=fail'
fi

printf 'VR_PATH=%s\n' "$PATH"

pi_command_path="$(command -v pi 2>/dev/null || true)"
printf 'VR_PI_COMMAND_PATH=%s\n' "$pi_command_path"

if command -v python >/dev/null 2>/dev/null && command -v python3 >/dev/null 2>/dev/null && python3 -m pip --version >/dev/null 2>/dev/null && python3 -m venv --help >/dev/null 2>/dev/null; then
  echo 'VR_CHECK python=pass'
else
  echo 'VR_CHECK python=fail'
fi
python_version="$(python3 --version 2>/dev/null || true)"
printf 'VR_PYTHON_VERSION=%s\n' "$python_version"

if node -e "fetch('https://pi.dev').then(r => process.exit(r.status > 0 ? 0 : 1)).catch(() => process.exit(1))" >/dev/null 2>/dev/null; then
  echo 'VR_CHECK internet=pass'
else
  echo 'VR_CHECK internet=fail'
fi

printf 'VR_GIT_NAME=%s\n' "${{GIT_AUTHOR_NAME:-}}"
printf 'VR_GIT_EMAIL=%s\n' "${{GIT_AUTHOR_EMAIL:-}}"
"#,
    );
    let output = compose_shell_output_without_ssh(config, &script)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(anyhow!("container doctor probe failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let container_path = line_value(&stdout, "VR_PATH=")
        .map(str::trim)
        .unwrap_or_default()
        .to_owned();
    let npm_global_bin = format!("{pi_npm_global_path}/bin");
    Ok(ContainerDoctorProbe {
        pi_config_writable: check_passed(&stdout, "pi_config_writable"),
        pi_sessions_writable: check_passed(&stdout, "pi_sessions_writable"),
        pi_npm_global_writable: check_passed(&stdout, "pi_npm_global_writable"),
        npm_global_bin_on_path: path_contains_entry(&container_path, &npm_global_bin),
        container_path,
        pi_command_path: line_value(&stdout, "VR_PI_COMMAND_PATH=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        python_available: check_passed(&stdout, "python"),
        python_version: line_value(&stdout, "VR_PYTHON_VERSION=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        internet_reachable: check_passed(&stdout, "internet"),
        git_identity: git_identity::git_identity_from_parts(
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

fn check_passed(output: &str, name: &str) -> bool {
    let prefix = format!("VR_CHECK {name}=");
    line_value(output, &prefix)
        .map(|value| value.trim() == "pass")
        .unwrap_or(false)
}

fn line_value<'a>(output: &'a str, prefix: &str) -> Option<&'a str> {
    output.lines().find_map(|line| line.strip_prefix(prefix))
}

fn path_contains_entry(path: &str, entry: &str) -> bool {
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
VR_CHECK pi_config_writable=pass
VR_CHECK pi_npm_global_writable=pass
VR_PATH=/home/agent/.npm-global/bin:/usr/local/bin:/usr/bin
VR_PI_COMMAND_PATH=/home/agent/.npm-global/bin/pi
VR_CHECK python=pass
VR_PYTHON_VERSION=Python 3.11.2
VR_CHECK internet=fail
VR_SSH_ADD_STDOUT=one
VR_SSH_ADD_STDOUT=two
VR_SSH_ADD_CODE=1
";

        assert!(check_passed(output, "pi_config_writable"));
        assert!(check_passed(output, "pi_npm_global_writable"));
        assert!(path_contains_entry(
            line_value(output, "VR_PATH=").unwrap(),
            "/home/agent/.npm-global/bin"
        ));
        assert_eq!(
            line_value(output, "VR_PI_COMMAND_PATH="),
            Some("/home/agent/.npm-global/bin/pi")
        );
        assert!(check_passed(output, "python"));
        assert_eq!(
            line_value(output, "VR_PYTHON_VERSION="),
            Some("Python 3.11.2")
        );
        assert!(!check_passed(output, "internet"));
        assert_eq!(line_value(output, "VR_SSH_ADD_CODE="), Some("1"));
        assert_eq!(prefixed_lines(output, "VR_SSH_ADD_STDOUT="), "one\ntwo");
    }
}
