use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result};

use crate::{
    alert,
    config::{Config, SelectedSshKey},
    harness,
    paths::display_path,
};

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

pub(super) fn prepare_override(
    config: &Config,
    runtime_dir: &Path,
    warn: bool,
) -> Result<Option<PathBuf>> {
    let Some(identity) = effective_git_identity(config) else {
        if warn {
            eprintln!(
                "{}: no Git identity configured or inherited; commits may fall back to the container user. Set git.user_name/git.user_email in ~/.vegasroom/config.yaml.",
                alert::warn()
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
  {service_name}:
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
        service_name = harness::PI.service_name,
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

pub(super) fn git_identity_from_parts(
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

#[cfg(test)]
mod tests {
    use super::*;

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
