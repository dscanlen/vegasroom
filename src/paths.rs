use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use directories::BaseDirs;

use crate::config::DEFAULT_CONFIG_YAML;

#[derive(Debug, Clone)]
pub struct StatePaths {
    pub root: PathBuf,
    pub harness_root: PathBuf,
    pub pi_root: PathBuf,
    pub pi_config: PathBuf,
    pub pi_extensions: PathBuf,
    pub pi_skills: PathBuf,
    pub pi_sessions: PathBuf,
    pub pi_auth_json: PathBuf,
    pub workspace: PathBuf,
    pub ssh_dir: PathBuf,
    pub known_hosts: PathBuf,
    pub cache: PathBuf,
    pub config_yaml: PathBuf,
    pub disclaimer_ack: PathBuf,
}

#[derive(Debug, Default)]
pub struct EnsureReport {
    created: Vec<PathBuf>,
    existing: Vec<PathBuf>,
}

impl StatePaths {
    pub fn default() -> Result<Self> {
        let base_dirs = BaseDirs::new().context("could not determine home directory")?;
        let root = base_dirs.home_dir().join(".vegasroom");
        Ok(Self::from_root(root))
    }

    pub fn from_root(root: PathBuf) -> Self {
        let harness_root = root.join("harness");
        let pi_root = harness_root.join("pi");
        let pi_config = pi_root.join("config");
        let pi_extensions = pi_root.join("extensions");
        let pi_skills = pi_root.join("skills");
        let pi_sessions = pi_root.join("sessions");
        let pi_auth_json = pi_config.join("auth.json");
        let workspace = root.join("workspace");
        let ssh_dir = root.join("ssh");
        let known_hosts = ssh_dir.join("known_hosts");
        let cache = root.join("cache");
        let config_yaml = root.join("config.yaml");
        let disclaimer_ack = cache.join("disclaimer-ack");

        Self {
            root,
            harness_root,
            pi_root,
            pi_config,
            pi_extensions,
            pi_skills,
            pi_sessions,
            pi_auth_json,
            workspace,
            ssh_dir,
            known_hosts,
            cache,
            config_yaml,
            disclaimer_ack,
        }
    }

    pub fn ensure(&self) -> Result<EnsureReport> {
        let mut report = EnsureReport::default();

        for dir in self.required_dirs() {
            ensure_dir(&dir, &mut report)?;
        }

        ensure_file(&self.config_yaml, DEFAULT_CONFIG_YAML, &mut report)?;
        ensure_file(&self.known_hosts, "", &mut report)?;

        Ok(report)
    }

    pub fn required_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.root.clone(),
            self.harness_root.clone(),
            self.pi_root.clone(),
            self.pi_config.clone(),
            self.pi_extensions.clone(),
            self.pi_skills.clone(),
            self.pi_sessions.clone(),
            self.workspace.clone(),
            self.ssh_dir.clone(),
            self.cache.clone(),
        ]
    }

    pub fn show_disclaimer_once(&self) -> Result<()> {
        if self.disclaimer_ack.exists() {
            return Ok(());
        }

        fs::create_dir_all(&self.cache).with_context(|| {
            format!("failed to create cache directory: {}", self.cache.display())
        })?;

        println!("Vegasroom launches AI agent harnesses inside ephemeral Docker containers.\n");
        println!("Only configured mounts persist. Your workspace and harness config are mounted read-write.\n");
        println!("Your SSH private keys are not copied into the container, but the forwarded ssh-agent socket can authorize SSH operations while mounted.\n");
        println!("Provider login state may persist inside the Pi harness mount after you use Pi /login.\n");
        println!("Default harness: Pi. Other harnesses can be added in future versions.\n");

        fs::write(&self.disclaimer_ack, "acknowledged\n").with_context(|| {
            format!(
                "failed to write disclaimer acknowledgement: {}",
                self.disclaimer_ack.display()
            )
        })?;

        Ok(())
    }
}

impl EnsureReport {
    pub fn print(&self) {
        if self.created.is_empty() {
            println!("No repairs needed.");
        } else {
            println!("Created or repaired:");
            for path in &self.created {
                println!("  {}", display_path(path));
            }
        }

        if !self.existing.is_empty() {
            println!("Already present:");
            for path in &self.existing {
                println!("  {}", display_path(path));
            }
        }
    }
}

pub fn expand_tilde(value: &str) -> PathBuf {
    if value == "~" {
        if let Some(base_dirs) = BaseDirs::new() {
            return base_dirs.home_dir().to_path_buf();
        }
    }

    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(base_dirs) = BaseDirs::new() {
            return base_dirs.home_dir().join(rest);
        }
    }

    PathBuf::from(value)
}

pub fn display_path(path: &Path) -> String {
    if let Some(base_dirs) = BaseDirs::new() {
        if let Ok(stripped) = path.strip_prefix(base_dirs.home_dir()) {
            return format!("~/{}", stripped.display());
        }
    }

    path.display().to_string()
}

fn ensure_dir(path: &Path, report: &mut EnsureReport) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            bail!(
                "Expected directory path exists as a file: {}\nRemove or rename it, then run: vr init",
                display_path(path)
            );
        }
        report.existing.push(path.to_path_buf());
        return Ok(());
    }

    fs::create_dir_all(path)
        .with_context(|| format!("failed to create directory: {}", display_path(path)))?;
    report.created.push(path.to_path_buf());
    Ok(())
}

fn ensure_file(path: &Path, contents: &str, report: &mut EnsureReport) -> Result<()> {
    if path.exists() {
        if !path.is_file() {
            bail!(
                "Expected file path exists as a directory: {}\nRemove or rename it, then run: vr init",
                display_path(path)
            );
        }
        report.existing.push(path.to_path_buf());
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create parent directory: {}",
                display_path(parent)
            )
        })?;
    }

    fs::write(path, contents)
        .with_context(|| format!("failed to create file: {}", display_path(path)))?;
    report.created.push(path.to_path_buf());
    Ok(())
}
