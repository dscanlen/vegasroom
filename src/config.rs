use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    atomic_write, harness,
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
  go:
    enabled: false
  typescript:
    enabled: false
    packages:
      - typescript
      - ts-node

harness:
  pi:
    image: vegasroom/pi:latest
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

    #[serde(default)]
    pub go: GoEnvironmentConfig,

    #[serde(default)]
    pub typescript: TypeScriptEnvironmentConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoEnvironmentConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeScriptEnvironmentConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_typescript_packages")]
    pub packages: Vec<String>,
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

/// Per-harness instance config, keyed by harness id in YAML (`harness.<id>`).
///
/// Deserialised through [`RawHarnessConfig`] so that every missing field is
/// filled from the matching [`harness::HarnessDescriptor`], never from another
/// harness's defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "RawHarnessConfig")]
pub struct HarnessConfig {
    pub pi: HarnessInstanceConfig,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HarnessInstanceConfig {
    pub image: String,
    pub command: String,
    pub network: String,
    pub build_network: String,
    pub read_only_workspace: bool,
    pub read_only_rootfs: bool,
}

#[derive(Debug, Default, Deserialize)]
struct RawHarnessConfig {
    #[serde(default)]
    pi: RawHarnessInstanceConfig,
}

#[derive(Debug, Default, Deserialize)]
struct RawHarnessInstanceConfig {
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    network: Option<String>,
    #[serde(default)]
    build_network: Option<String>,
    #[serde(default)]
    read_only_workspace: Option<bool>,
    #[serde(default)]
    read_only_rootfs: Option<bool>,
}

impl RawHarnessInstanceConfig {
    fn resolve(self, descriptor: &harness::HarnessDescriptor) -> HarnessInstanceConfig {
        let defaults = HarnessInstanceConfig::defaults_for(descriptor);
        HarnessInstanceConfig {
            image: self.image.unwrap_or(defaults.image),
            command: self.command.unwrap_or(defaults.command),
            network: self.network.unwrap_or(defaults.network),
            build_network: self.build_network.unwrap_or(defaults.build_network),
            read_only_workspace: self
                .read_only_workspace
                .unwrap_or(defaults.read_only_workspace),
            read_only_rootfs: self.read_only_rootfs.unwrap_or(defaults.read_only_rootfs),
        }
    }
}

impl From<RawHarnessConfig> for HarnessConfig {
    fn from(raw: RawHarnessConfig) -> Self {
        Self {
            pi: raw.pi.resolve(&harness::PI),
        }
    }
}

impl Default for HarnessConfig {
    fn default() -> Self {
        RawHarnessConfig::default().into()
    }
}

impl HarnessConfig {
    /// Instance config for a registered harness. Returns `None` for an id with no config field.
    pub fn try_instance(
        &self,
        descriptor: &harness::HarnessDescriptor,
    ) -> Option<&HarnessInstanceConfig> {
        match descriptor.id {
            id if id == harness::PI.id => Some(&self.pi),
            _ => None,
        }
    }

    /// Instance config for a registered harness.
    ///
    /// # Panics
    /// If the descriptor has no matching config field. Every entry in
    /// `harness::HARNESSES` must have one; this is enforced by tests.
    pub fn instance(&self, descriptor: &harness::HarnessDescriptor) -> &HarnessInstanceConfig {
        self.try_instance(descriptor)
            .unwrap_or_else(|| panic!("no harness config field for harness id `{}`", descriptor.id))
    }
}

impl HarnessInstanceConfig {
    pub fn defaults_for(descriptor: &harness::HarnessDescriptor) -> Self {
        Self {
            image: descriptor.default_image.to_owned(),
            command: descriptor.default_command.to_owned(),
            network: default_network(),
            build_network: default_network(),
            read_only_workspace: false,
            read_only_rootfs: false,
        }
    }
}

pub struct HarnessRuntimeConfig<'a> {
    pub image: &'a str,
    pub command: &'a str,
}

impl Config {
    pub fn harness_runtime_config<'a>(
        &'a self,
        descriptor: &harness::HarnessDescriptor,
    ) -> HarnessRuntimeConfig<'a> {
        let instance = self.harness.instance(descriptor);
        HarnessRuntimeConfig {
            image: &instance.image,
            command: &instance.command,
        }
    }

    pub fn load_or_default() -> Result<Self> {
        Self::load_with(&StatePaths::default()?)
    }

    pub fn load_with(state: &StatePaths) -> Result<Self> {
        if state.config_yaml.exists() {
            Self::load_from_path(state.config_yaml.clone())
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

    pub fn save_to_default_path_with(&self, state: &StatePaths) -> Result<()> {
        self.save_to_path(&state.config_yaml)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        self.validate_semantics()?;
        let contents = serde_yaml::to_string(self).context("failed to serialize config")?;
        atomic_write::write_file(path, contents)
            .with_context(|| format!("failed to write config: {}", display_path(path)))
    }

    pub fn validate_semantics(&self) -> Result<()> {
        validate_non_empty("paths.workspace", &self.paths.workspace)?;
        validate_non_empty("docker.context", &self.docker.context)?;
        validate_non_empty("docker.compose_file", &self.docker.compose_file)?;
        for descriptor in harness::HARNESSES {
            validate_harness_instance(descriptor.id, self.harness.instance(descriptor))?;
        }
        validate_environment_config(&self.environment)?;
        Ok(())
    }

    pub fn compose_file_path(&self) -> PathBuf {
        expand_tilde(&self.docker.compose_file)
    }

    pub fn resolved_compose_file(&self) -> Result<PathBuf> {
        self.resolved_compose_file_from_state(&StatePaths::default()?)
    }

    pub(crate) fn resolved_compose_file_from_state(&self, state: &StatePaths) -> Result<PathBuf> {
        if state.runtime_compose_yaml.is_file() {
            return state.runtime_compose_yaml.canonicalize().with_context(|| {
                format!(
                    "failed to canonicalize managed Compose file: {}",
                    display_path(&state.runtime_compose_yaml)
                )
            });
        }

        bail!(
            "Managed Compose runtime file was not found: {}\nRun `vr init` to write the managed runtime files.",
            display_path(&state.runtime_compose_yaml)
        );
    }

    pub fn uses_managed_compose_file_with(&self, state: &StatePaths) -> bool {
        self.compose_file_path() == state.runtime_compose_yaml
    }

    pub fn set_managed_compose_file_with(&mut self, state: &StatePaths) -> Result<()> {
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

fn default_typescript_packages() -> Vec<String> {
    vec!["typescript".to_owned(), "ts-node".to_owned()]
}

impl Default for TypeScriptEnvironmentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            packages: default_typescript_packages(),
        }
    }
}

fn validate_environment_config(environment: &EnvironmentConfig) -> Result<()> {
    for package in normalized_apt_packages(environment) {
        if !is_safe_apt_package_name(&package) {
            bail!(
                "invalid apt package name in environment.apt.packages: {package}\nPackage names may contain only ASCII letters, digits, '.', '+', '-', ':', and '_'"
            );
        }
    }

    if environment.rust.enabled {
        let toolchain = normalized_rust_toolchain(environment);
        if !is_safe_rust_toolchain(&toolchain) {
            bail!(
                "invalid Rust toolchain in environment.rust.toolchain: {toolchain}\nToolchains may contain only ASCII letters, digits, '.', '-', and '_'"
            );
        }

        for component in normalized_rust_components(environment) {
            if !is_safe_rust_component(&component) {
                bail!(
                    "invalid Rust component in environment.rust.components: {component}\nComponents may contain only ASCII letters, digits, '-', and '_'"
                );
            }
        }
    }

    if environment.typescript.enabled {
        let packages = normalized_typescript_packages(environment);
        if packages.is_empty() {
            bail!("environment.typescript.enabled is true but no npm packages are configured");
        }

        for package in packages {
            if !is_safe_npm_package_name(&package) {
                bail!(
                    "invalid npm package in environment.typescript.packages: {package}\nPackage names may contain only ASCII letters, digits, '.', '+', '-', '_', '/', and one leading '@' for scoped packages"
                );
            }
        }
    }

    Ok(())
}

pub(crate) fn normalized_apt_packages(environment: &EnvironmentConfig) -> Vec<String> {
    normalized_unique(&environment.apt.packages)
}

pub(crate) fn normalized_rust_toolchain(environment: &EnvironmentConfig) -> String {
    let toolchain = environment.rust.toolchain.trim();
    if toolchain.is_empty() {
        default_rust_toolchain()
    } else {
        toolchain.to_owned()
    }
}

pub(crate) fn normalized_rust_components(environment: &EnvironmentConfig) -> Vec<String> {
    normalized_unique(&environment.rust.components)
}

pub(crate) fn normalized_typescript_packages(environment: &EnvironmentConfig) -> Vec<String> {
    normalized_unique(&environment.typescript.packages)
}

fn normalized_unique(values: &[String]) -> Vec<String> {
    let mut values = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn validate_harness_instance(id: &str, instance: &HarnessInstanceConfig) -> Result<()> {
    validate_docker_reference(&format!("harness.{id}.image"), &instance.image)?;
    validate_non_empty(&format!("harness.{id}.command"), &instance.command)?;
    validate_shell_free_value(&format!("harness.{id}.command"), &instance.command)?;
    validate_docker_network(&format!("harness.{id}.network"), &instance.network)?;
    validate_docker_network(
        &format!("harness.{id}.build_network"),
        &instance.build_network,
    )?;
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(())
}

fn validate_docker_reference(field: &str, value: &str) -> Result<()> {
    validate_non_empty(field, value)?;
    validate_shell_free_value(field, value)
}

fn validate_docker_network(field: &str, value: &str) -> Result<()> {
    validate_non_empty(field, value)?;
    validate_shell_free_value(field, value)
}

fn validate_shell_free_value(field: &str, value: &str) -> Result<()> {
    if value.chars().any(char::is_whitespace) || value.chars().any(char::is_control) {
        bail!("{field} must not contain whitespace or control characters: {value:?}");
    }
    Ok(())
}

fn is_safe_apt_package_name(package: &str) -> bool {
    !package.is_empty()
        && package.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-' | b':' | b'_')
        })
}

fn is_safe_rust_toolchain(toolchain: &str) -> bool {
    !toolchain.is_empty()
        && toolchain
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn is_safe_rust_component(component: &str) -> bool {
    !component.is_empty()
        && component
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn is_safe_npm_package_name(package: &str) -> bool {
    if package.is_empty() || package.contains("//") {
        return false;
    }

    let at_count = package.bytes().filter(|byte| *byte == b'@').count();
    if at_count > 1 || at_count == 1 && !package.starts_with('@') {
        return false;
    }

    package.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(byte, b'.' | b'+' | b'-' | b'_' | b'/')
            || byte == b'@'
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::unique_temp_dir;

    #[test]
    fn every_registered_harness_has_an_instance_config() {
        let config = Config::default();
        for descriptor in harness::HARNESSES {
            let instance = config
                .harness
                .try_instance(descriptor)
                .unwrap_or_else(|| panic!("missing harness.{} config field", descriptor.id));
            assert_eq!(instance, &HarnessInstanceConfig::defaults_for(descriptor));
        }
    }

    #[test]
    fn partial_harness_block_fills_missing_fields_from_its_descriptor() {
        let config: Config = serde_yaml::from_str(
            "harness:\n  pi:\n    network: bridge\n    read_only_rootfs: true\n",
        )
        .unwrap();

        let pi = config.harness.instance(&harness::PI);
        assert_eq!(pi.network, "bridge");
        assert!(pi.read_only_rootfs);
        assert_eq!(pi.image, harness::PI.default_image);
        assert_eq!(pi.command, harness::PI.default_command);
        assert_eq!(pi.build_network, "host");
        assert!(!pi.read_only_workspace);
    }

    #[test]
    fn missing_harness_section_uses_descriptor_defaults() {
        let config: Config = serde_yaml::from_str("ui:\n  color: auto\n").unwrap();
        assert_eq!(
            config.harness.instance(&harness::PI),
            &HarnessInstanceConfig::defaults_for(&harness::PI)
        );
    }

    #[test]
    fn harness_config_round_trips_through_yaml() {
        let mut config = Config::default();
        config.harness.pi.image = "example/pi:rt".to_owned();
        config.harness.pi.read_only_workspace = true;

        let yaml = serde_yaml::to_string(&config).unwrap();
        let reparsed: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(reparsed.harness.pi, config.harness.pi);
    }

    #[test]
    fn runtime_config_reads_from_descriptor_instance() {
        let mut config = Config::default();
        config.harness.pi.image = "example/pi:rc".to_owned();
        config.harness.pi.command = "pi-custom".to_owned();

        let runtime = config.harness_runtime_config(&harness::PI);
        assert_eq!(runtime.image, "example/pi:rc");
        assert_eq!(runtime.command, "pi-custom");
    }

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
        assert!(!config.environment.go.enabled);
        assert!(!config.environment.typescript.enabled);
        assert_eq!(
            config.environment.typescript.packages,
            vec!["typescript".to_owned(), "ts-node".to_owned()]
        );
        assert_eq!(config.harness.pi.image, "vegasroom/pi:latest");
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
        assert!(!config.environment.go.enabled);
        assert!(!config.environment.typescript.enabled);
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
    fn environment_go_config_is_parsed() {
        let config: Config = serde_yaml::from_str(
            r#"environment:
  go:
    enabled: true
"#,
        )
        .unwrap();

        assert!(config.environment.go.enabled);
    }

    #[test]
    fn environment_typescript_config_is_parsed() {
        let config: Config = serde_yaml::from_str(
            r#"environment:
  typescript:
    enabled: true
    packages:
      - typescript
"#,
        )
        .unwrap();

        assert!(config.environment.typescript.enabled);
        assert_eq!(
            config.environment.typescript.packages,
            vec!["typescript".to_owned()]
        );
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
    fn semantic_validation_accepts_default_config() {
        Config::default().validate_semantics().unwrap();
    }

    #[test]
    fn semantic_validation_rejects_invalid_package_names() {
        let mut config = Config::default();
        config.environment.apt.packages = vec!["bad;package".to_owned()];

        let err = config.validate_semantics().unwrap_err();

        assert!(err.to_string().contains("invalid apt package name"));
    }

    #[test]
    fn semantic_validation_rejects_invalid_rust_values() {
        let mut config = Config::default();
        config.environment.rust.enabled = true;
        config.environment.rust.toolchain = "bad toolchain".to_owned();

        let err = config.validate_semantics().unwrap_err();

        assert!(err.to_string().contains("invalid Rust toolchain"));

        config.environment.rust.toolchain = "stable".to_owned();
        config.environment.rust.components = vec!["bad;component".to_owned()];

        let err = config.validate_semantics().unwrap_err();

        assert!(err.to_string().contains("invalid Rust component"));
    }

    #[test]
    fn semantic_validation_rejects_invalid_npm_package_names() {
        let mut config = Config::default();
        config.environment.typescript.enabled = true;
        config.environment.typescript.packages = vec!["bad;package".to_owned()];

        let err = config.validate_semantics().unwrap_err();

        assert!(err.to_string().contains("invalid npm package"));
    }

    #[test]
    fn semantic_validation_rejects_empty_typescript_package_set() {
        let mut config = Config::default();
        config.environment.typescript.enabled = true;
        config.environment.typescript.packages.clear();

        let err = config.validate_semantics().unwrap_err();

        assert!(err.to_string().contains("no npm packages"));
    }

    #[test]
    fn semantic_validation_rejects_whitespace_in_docker_values() {
        let mut config = Config::default();
        config.harness.pi.image = "bad image".to_owned();

        let err = config.validate_semantics().unwrap_err();

        assert!(err.to_string().contains("harness.pi.image"));
    }

    #[test]
    fn save_rejects_semantically_invalid_config() {
        let path = std::env::temp_dir().join(format!(
            "vegasroom-invalid-config-{}-{}.yaml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut config = Config::default();
        config.docker.context = "".to_owned();

        let err = config.save_to_path(&path).unwrap_err();

        assert!(err.to_string().contains("docker.context"));
        assert!(!path.exists());
    }

    #[test]
    fn resolved_compose_file_uses_managed_runtime_even_when_custom_file_exists() {
        let root = unique_temp_dir("managed-compose");
        let state = StatePaths::from_root(root.clone());
        std::fs::create_dir_all(&state.runtime_root).unwrap();
        std::fs::write(&state.runtime_compose_yaml, "managed\n").unwrap();
        let custom = root.join("custom.compose.yaml");
        std::fs::write(&custom, "custom\n").unwrap();

        let mut config = Config::default();
        config.docker.compose_file = custom.display().to_string();

        let resolved = config.resolved_compose_file_from_state(&state).unwrap();

        assert_eq!(resolved, state.runtime_compose_yaml.canonicalize().unwrap());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn resolved_compose_file_reports_missing_managed_runtime() {
        let root = unique_temp_dir("missing-managed-compose");
        let state = StatePaths::from_root(root.clone());
        let config = Config::default();

        let err = config.resolved_compose_file_from_state(&state).unwrap_err();

        assert!(err.to_string().contains("Managed Compose runtime file"));
        assert!(err.to_string().contains("vr init"));

        let _ = std::fs::remove_dir_all(root);
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
