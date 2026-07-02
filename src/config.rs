use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::{display_path, expand_tilde, StatePaths};

pub const DEFAULT_CONFIG_YAML: &str = r#"default_harness: pi

paths:
  root: ~/.vegasroom
  workspace: ~/.vegasroom/workspace

docker:
  context: rootless
  compose_file: ~/.vegasroom/runtime/compose.yaml

ssh:
  mode: auto
  selected_keys: []

harness:
  pi:
    enabled: true
    image: vegasroom/pi:local
    command: pi
    ssh_agent: auto
    network: host

  # claude:
  #   enabled: false
  #   image: vegasroom/claude:local
  #   command: claude
  #   ssh_agent: auto
  #   network: host
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_harness")]
    pub default_harness: String,

    #[serde(default)]
    pub paths: PathsConfig,

    #[serde(default)]
    pub docker: DockerConfig,

    #[serde(default)]
    pub ssh: SshConfig,

    #[serde(default)]
    pub harness: HarnessConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    #[serde(default = "default_root")]
    pub root: String,

    #[serde(default = "default_workspace")]
    pub workspace: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HarnessConfig {
    #[serde(default)]
    pub pi: PiHarnessConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiHarnessConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_pi_image")]
    pub image: String,

    #[serde(default = "default_pi_command")]
    pub command: String,

    #[serde(default = "default_ssh_agent")]
    pub ssh_agent: String,

    #[serde(default = "default_network")]
    pub network: String,
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
            root: default_root(),
            workspace: default_workspace(),
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

impl Default for PiHarnessConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            image: default_pi_image(),
            command: default_pi_command(),
            ssh_agent: default_ssh_agent(),
            network: default_network(),
        }
    }
}

fn default_harness() -> String {
    "pi".to_owned()
}

fn default_root() -> String {
    "~/.vegasroom".to_owned()
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
    "vegasroom/pi:local".to_owned()
}

fn default_pi_command() -> String {
    "pi".to_owned()
}

fn default_ssh_agent() -> String {
    "auto".to_owned()
}

fn default_network() -> String {
    "host".to_owned()
}
