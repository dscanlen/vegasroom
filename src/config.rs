use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    harness,
    paths::{display_path, expand_tilde, StatePaths},
};

pub const DEFAULT_CONFIG_YAML: &str = r#"paths:
  workspace: ~/.vegasroom/workspace

workspace:
  risky_mount_policy: warn

docker:
  context: rootless
  compose_file: ~/.vegasroom/runtime/compose.yaml

ssh:
  mode: auto
  selected_keys: []

git:
  inherit_host: true
  user_name:
  user_email:

ui:
  color: auto

environment:
  apt:
    packages: []
  rust:
    enabled: false
    toolchain: stable
    components:
      - rustfmt
      - clippy
  python:
    enabled: false

harness:
  pi:
    image: vegasroom/pi:local
    command: pi
    network: host
    build_network: host
    read_only_workspace: false
    read_only_rootfs: false
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub paths: PathsConfig,

    #[serde(default)]
    pub workspace: WorkspaceConfig,

    #[serde(default)]
    pub docker: DockerConfig,

    #[serde(default)]
    pub ssh: SshConfig,

    #[serde(default)]
    pub git: GitConfig,

    #[serde(default)]
    pub ui: UiConfig,

    #[serde(default)]
    pub environment: EnvironmentConfig,

    #[serde(default)]
    pub harness: HarnessConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    #[serde(default = "default_workspace")]
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub risky_mount_policy: RiskyMountPolicy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RiskyMountPolicy {
    #[default]
    Warn,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerConfig {
    #[serde(default = "default_context")]
    pub context: String,

    #[serde(default = "default_compose_file")]
    pub compose_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    #[serde(default)]
    pub mode: SshMode,

    #[serde(default)]
    pub selected_keys: Vec<SelectedSshKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    #[serde(default = "default_true")]
    pub inherit_host: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiConfig {
    #[serde(default)]
    pub color: ColorMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvironmentConfig {
    #[serde(default)]
    pub apt: AptEnvironmentConfig,

    #[serde(default)]
    pub rust: RustEnvironmentConfig,

    #[serde(default)]
    pub python: PythonEnvironmentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AptEnvironmentConfig {
    #[serde(default)]
    pub packages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustEnvironmentConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_rust_toolchain")]
    pub toolchain: String,

    #[serde(default = "default_rust_components")]
    pub components: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PythonEnvironmentConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SshMode {
    #[default]
    Auto,
    Host,
    Managed,
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectedSshKey {
    pub path: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_type: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_user_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_user_email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HarnessConfig {
    #[serde(default)]
    pub pi: PiHarnessConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiHarnessConfig {
    #[serde(default = "default_pi_image")]
    pub image: String,

    #[serde(default = "default_pi_command")]
    pub command: String,

    #[serde(default = "default_network")]
    pub network: String,

    #[serde(default = "default_network")]
    pub build_network: String,

    #[serde(default)]
    pub read_only_workspace: bool,

    #[serde(default)]
    pub read_only_rootfs: bool,
}

impl Config {
    pub fn load_or_default() -> Result<Self> {
        let paths = StatePaths::default()?;
        if paths.config_yaml.exists() {
            Self::load_from_path(paths.config_yaml)
        } else {
            serde_yaml::from_str(DEFAULT_CONFIG_YAML)
                .context("failed to parse built-in default config")
        }
    }

    pub fn load_from_path(path: PathBuf) -> Result<Self> {
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse config: {}", path.display()))
    }

    pub fn save_to_default_path(&self) -> Result<()> {
        let paths = StatePaths::default()?;
        self.save_to_path(&paths.config_yaml)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        let contents = serde_yaml::to_string(self).context("failed to serialize config")?;
        fs::write(path, contents)
            .with_context(|| format!("failed to write config: {}", display_path(path)))
    }

    pub fn compose_file_path(&self) -> PathBuf {
        expand_tilde(&self.docker.compose_file)
    }

    pub fn resolved_compose_file(&self) -> Result<PathBuf> {
        let configured = self.compose_file_path();
        if configured.is_file() {
            return configured.canonicalize().with_context(|| {
                format!(
                    "failed to canonicalize Compose file: {}",
                    display_path(&configured)
                )
            });
        }

        let state = StatePaths::default()?;
        if state.runtime_compose_yaml.is_file() {
            return state.runtime_compose_yaml.canonicalize().with_context(|| {
                format!(
                    "failed to canonicalize managed Compose file: {}",
                    display_path(&state.runtime_compose_yaml)
                )
            });
        }

        bail!(
            "Compose runtime file was not found: {}\nRun `vr init` to write the managed runtime files.",
            display_path(&configured)
        );
    }

    pub fn uses_managed_compose_file(&self) -> Result<bool> {
        let state = StatePaths::default()?;
        Ok(self.compose_file_path() == state.runtime_compose_yaml)
    }

    pub fn set_managed_compose_file(&mut self) -> Result<()> {
        let state = StatePaths::default()?;
        self.docker.compose_file = display_path(&state.runtime_compose_yaml);
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        serde_yaml::from_str(DEFAULT_CONFIG_YAML)
            .expect("built-in default config should always parse")
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            workspace: default_workspace(),
        }
    }
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            risky_mount_policy: RiskyMountPolicy::Warn,
        }
    }
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            context: default_context(),
            compose_file: default_compose_file(),
        }
    }
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            mode: SshMode::Auto,
            selected_keys: Vec::new(),
        }
    }
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            inherit_host: true,
            user_name: None,
            user_email: None,
        }
    }
}

impl Default for PiHarnessConfig {
    fn default() -> Self {
        Self {
            image: default_pi_image(),
            command: default_pi_command(),
            network: default_network(),
            build_network: default_network(),
            read_only_workspace: false,
            read_only_rootfs: false,
        }
    }
}

fn default_workspace() -> String {
    "~/.vegasroom/workspace".to_owned()
}

fn default_context() -> String {
    "rootless".to_owned()
}

fn default_compose_file() -> String {
    "~/.vegasroom/runtime/compose.yaml".to_owned()
}

fn default_true() -> bool {
    true
}

fn default_pi_image() -> String {
    harness::PI.default_image.to_owned()
}

fn default_pi_command() -> String {
    harness::PI.default_command.to_owned()
}

fn default_network() -> String {
    "host".to_owned()
}

fn default_rust_toolchain() -> String {
    "stable".to_owned()
}

fn default_rust_components() -> Vec<String> {
    vec!["rustfmt".to_owned(), "clippy".to_owned()]
}

impl Default for RustEnvironmentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            toolchain: default_rust_toolchain(),
            components: default_rust_components(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_default_config_parses() {
        let config: Config = serde_yaml::from_str(DEFAULT_CONFIG_YAML).unwrap();

        assert_eq!(config.paths.workspace, "~/.vegasroom/workspace");
        assert_eq!(config.workspace.risky_mount_policy, RiskyMountPolicy::Warn);
        assert_eq!(config.docker.context, "rootless");
        assert_eq!(
            config.docker.compose_file,
            "~/.vegasroom/runtime/compose.yaml"
        );
        assert_eq!(config.ssh.mode, SshMode::Auto);
        assert!(config.ssh.selected_keys.is_empty());
        assert!(config.git.inherit_host);
        assert!(config.git.user_name.is_none());
        assert!(config.git.user_email.is_none());
        assert_eq!(config.ui.color, ColorMode::Auto);
        assert!(config.environment.apt.packages.is_empty());
        assert!(!config.environment.rust.enabled);
        assert_eq!(config.environment.rust.toolchain, "stable");
        assert_eq!(
            config.environment.rust.components,
            vec!["rustfmt".to_owned(), "clippy".to_owned()]
        );
        assert!(!config.environment.python.enabled);
        assert_eq!(config.harness.pi.image, "vegasroom/pi:local");
        assert_eq!(config.harness.pi.command, "pi");
        assert_eq!(config.harness.pi.network, "host");
        assert_eq!(config.harness.pi.build_network, "host");
        assert!(!config.harness.pi.read_only_workspace);
        assert!(!config.harness.pi.read_only_rootfs);
    }

    #[test]
    fn legacy_future_fields_are_accepted_but_not_serialized() {
        let config: Config = serde_yaml::from_str(
            r#"default_harness: claude
paths:
  root: /tmp/ignored
  workspace: /tmp/workspace
harness:
  pi:
    enabled: false
    ssh_agent: off
    image: example/pi:test
"#,
        )
        .unwrap();

        assert_eq!(config.paths.workspace, "/tmp/workspace");
        assert_eq!(config.harness.pi.image, "example/pi:test");

        let serialized = serde_yaml::to_string(&config).unwrap();
        assert!(!serialized.contains("default_harness"));
        assert!(!serialized.contains("root:"));
        assert!(!serialized.contains("  pi:\n    enabled:"));
        assert!(!serialized.contains("ssh_agent:"));
    }

    #[test]
    fn partial_config_uses_defaults_for_missing_sections() {
        let config: Config = serde_yaml::from_str("docker:\n  context: test-context\n").unwrap();

        assert_eq!(config.docker.context, "test-context");
        assert_eq!(config.paths.workspace, "~/.vegasroom/workspace");
        assert_eq!(config.workspace.risky_mount_policy, RiskyMountPolicy::Warn);
        assert_eq!(config.ssh.mode, SshMode::Auto);
        assert_eq!(config.ui.color, ColorMode::Auto);
        assert!(config.environment.apt.packages.is_empty());
        assert!(!config.environment.rust.enabled);
        assert!(!config.environment.python.enabled);
        assert_eq!(config.harness.pi.command, "pi");
        assert_eq!(config.harness.pi.build_network, "host");
        assert!(!config.harness.pi.read_only_workspace);
        assert!(!config.harness.pi.read_only_rootfs);
    }

    #[test]
    fn workspace_risky_mount_policy_config_is_parsed() {
        let config: Config = serde_yaml::from_str(
            r#"workspace:
  risky_mount_policy: deny
"#,
        )
        .unwrap();

        assert_eq!(config.workspace.risky_mount_policy, RiskyMountPolicy::Deny);
    }

    #[test]
    fn environment_apt_packages_config_is_parsed() {
        let config: Config = serde_yaml::from_str(
            r#"environment:
  apt:
    packages:
      - build-essential
      - pkg-config
"#,
        )
        .unwrap();

        assert_eq!(
            config.environment.apt.packages,
            vec!["build-essential".to_owned(), "pkg-config".to_owned()]
        );
    }

    #[test]
    fn environment_rust_config_is_parsed() {
        let config: Config = serde_yaml::from_str(
            r#"environment:
  rust:
    enabled: true
    toolchain: nightly
    components:
      - rustfmt
"#,
        )
        .unwrap();

        assert!(config.environment.rust.enabled);
        assert_eq!(config.environment.rust.toolchain, "nightly");
        assert_eq!(
            config.environment.rust.components,
            vec!["rustfmt".to_owned()]
        );
    }

    #[test]
    fn environment_python_config_is_parsed() {
        let config: Config = serde_yaml::from_str(
            r#"environment:
  python:
    enabled: true
"#,
        )
        .unwrap();

        assert!(config.environment.python.enabled);
    }

    #[test]
    fn pi_read_only_workspace_config_is_parsed() {
        let config: Config = serde_yaml::from_str(
            r#"harness:
  pi:
    read_only_workspace: true
"#,
        )
        .unwrap();

        assert!(config.harness.pi.read_only_workspace);
    }

    #[test]
    fn pi_build_network_config_is_parsed_independently() {
        let config: Config = serde_yaml::from_str(
            r#"harness:
  pi:
    network: bridge
    build_network: host
"#,
        )
        .unwrap();

        assert_eq!(config.harness.pi.network, "bridge");
        assert_eq!(config.harness.pi.build_network, "host");
    }

    #[test]
    fn pi_read_only_rootfs_config_is_parsed() {
        let config: Config = serde_yaml::from_str(
            r#"harness:
  pi:
    read_only_rootfs: true
"#,
        )
        .unwrap();

        assert!(config.harness.pi.read_only_rootfs);
    }

    #[test]
    fn ui_color_config_is_parsed() {
        let config: Config = serde_yaml::from_str(
            r#"ui:
  color: never
"#,
        )
        .unwrap();

        assert_eq!(config.ui.color, ColorMode::Never);
    }

    #[test]
    fn config_round_trips_selected_key_metadata() {
        let mut config = Config::default();
        config.ssh.selected_keys.push(SelectedSshKey {
            path: "~/.ssh/id_ed25519".to_owned(),
            fingerprint: Some("SHA256:test".to_owned()),
            comment: Some("agent@example".to_owned()),
            key_type: Some("ED25519".to_owned()),
            git_user_name: Some("Agent User".to_owned()),
            git_user_email: Some("agent@example.com".to_owned()),
        });

        let serialized = serde_yaml::to_string(&config).unwrap();
        let reparsed: Config = serde_yaml::from_str(&serialized).unwrap();

        assert_eq!(reparsed.ssh.selected_keys, config.ssh.selected_keys);
    }
}
