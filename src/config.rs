use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::{expand_tilde, StatePaths};

pub const DEFAULT_CONFIG_YAML: &str = r#"default_harness: pi

paths:
  root: ~/.vegasroom
  workspace: ~/.vegasroom/workspace

docker:
  context: rootless
  compose_file: ./compose.yaml

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

    pub fn compose_file_path(&self) -> PathBuf {
        expand_tilde(&self.docker.compose_file)
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
    "./compose.yaml".to_owned()
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
