use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use directories::BaseDirs;

use crate::{assets, config::DEFAULT_CONFIG_YAML, harness};

#[derive(Debug, Clone)]
pub struct StatePaths {
    pub root: PathBuf,
    pub harness_root: PathBuf,
    pub pi_root: PathBuf,
    pub pi_config: PathBuf,
    pub pi_extensions: PathBuf,
    pub pi_skills: PathBuf,
    pub pi_sessions: PathBuf,
    pub pi_npm_global: PathBuf,
    pub pi_auth_json: PathBuf,
    pub workspace: PathBuf,
    pub environment_root: PathBuf,
    pub cargo_home: PathBuf,
    pub runtime_root: PathBuf,
    pub runtime_harness_root: PathBuf,
    pub runtime_pi_root: PathBuf,
    pub runtime_pi_dockerfile: PathBuf,
    pub runtime_compose_yaml: PathBuf,
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

#[cfg(unix)]
pub const PRIVATE_DIR_MODE: u32 = 0o700;

#[cfg(unix)]
pub const PRIVATE_FILE_MODE: u32 = 0o600;

impl StatePaths {
    pub fn default() -> Result<Self> {
        let base_dirs = BaseDirs::new().context("could not determine home directory")?;
        let root = base_dirs.home_dir().join(".vegasroom");
        Ok(Self::from_root(root))
    }

    pub fn from_root(root: PathBuf) -> Self {
        let harness_root = root.join("harness");
        let pi_root = harness_root.join(harness::PI.id);
        let pi_config = pi_root.join(harness::PI_CONFIG_DIR);
        let pi_extensions = pi_root.join(harness::PI_EXTENSIONS_DIR);
        let pi_skills = pi_root.join(harness::PI_SKILLS_DIR);
        let pi_sessions = pi_root.join(harness::PI_SESSIONS_DIR);
        let pi_npm_global = pi_root.join(harness::PI_NPM_GLOBAL_DIR);
        let pi_auth_json = pi_root.join(harness::PI.auth_state_relative_path);
        let workspace = root.join("workspace");
        let environment_root = root.join("environment");
        let cargo_home = environment_root.join("cargo");
        let runtime_root = root.join("runtime");
        let runtime_harness_root = runtime_root.join("harness");
        let runtime_pi_root = runtime_harness_root.join(harness::PI.id);
        let runtime_pi_dockerfile = runtime_root.join(harness::PI.dockerfile_path);
        let runtime_compose_yaml = runtime_root.join("compose.yaml");
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
            pi_npm_global,
            pi_auth_json,
            workspace,
            environment_root,
            cargo_home,
            runtime_root,
            runtime_harness_root,
            runtime_pi_root,
            runtime_pi_dockerfile,
            runtime_compose_yaml,
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
        write_managed_file(
            &self.runtime_compose_yaml,
            assets::MANAGED_COMPOSE_YAML,
            &mut report,
        )?;
        write_managed_file(
            &self.runtime_pi_dockerfile,
            assets::MANAGED_PI_DOCKERFILE,
            &mut report,
        )?;
        self.apply_private_permissions()?;

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
            self.pi_npm_global.clone(),
            self.workspace.clone(),
            self.environment_root.clone(),
            self.cargo_home.clone(),
            self.runtime_root.clone(),
            self.runtime_harness_root.clone(),
            self.runtime_pi_root.clone(),
            self.ssh_dir.clone(),
            self.cache.clone(),
        ]
    }

    pub fn private_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.root.clone(),
            self.harness_root.clone(),
            self.pi_root.clone(),
            self.pi_config.clone(),
            self.pi_extensions.clone(),
            self.pi_skills.clone(),
            self.pi_sessions.clone(),
            self.pi_npm_global.clone(),
            self.environment_root.clone(),
            self.cargo_home.clone(),
            self.runtime_root.clone(),
            self.runtime_harness_root.clone(),
            self.runtime_pi_root.clone(),
            self.ssh_dir.clone(),
            self.cache.clone(),
        ]
    }

    pub fn private_files(&self) -> Vec<PathBuf> {
        vec![
            self.config_yaml.clone(),
            self.known_hosts.clone(),
            self.runtime_compose_yaml.clone(),
            self.runtime_pi_dockerfile.clone(),
        ]
    }

    pub fn apply_private_permissions(&self) -> Result<()> {
        for dir in self.private_dirs() {
            if dir.exists() {
                set_private_dir_permissions(&dir)?;
            }
        }

        for file in self.private_files() {
            if file.exists() {
                set_private_file_permissions(&file)?;
            }
        }

        Ok(())
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
        println!(
            "Default harness: {}. Other harnesses can be added in future versions.\n",
            harness::PI.display_name
        );

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

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<()> {
    set_permissions_mode(path, PRIVATE_DIR_MODE, "directory")
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<()> {
    set_permissions_mode(path, PRIVATE_FILE_MODE, "file")
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_permissions_mode(path: &Path, mode: u32, kind: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path).with_context(|| {
        format!(
            "failed to read permissions for private {kind}: {}",
            display_path(path)
        )
    })?;
    let mut permissions = metadata.permissions();
    if permissions.mode() & 0o777 == mode {
        return Ok(());
    }

    permissions.set_mode(mode);
    fs::set_permissions(path, permissions).with_context(|| {
        format!(
            "failed to set private permissions on {kind}: {}",
            display_path(path)
        )
    })
}

fn write_managed_file(path: &Path, contents: &str, report: &mut EnsureReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent, report)?;
    }

    if path.exists() {
        if !path.is_file() {
            bail!(
                "Expected managed runtime file path exists as a directory: {}\nRemove or rename it, then run: vr init",
                display_path(path)
            );
        }

        let current = fs::read_to_string(path).with_context(|| {
            format!(
                "failed to read managed runtime file: {}",
                display_path(path)
            )
        })?;
        if current == contents {
            report.existing.push(path.to_path_buf());
            return Ok(());
        }
    }

    fs::write(path, contents).with_context(|| {
        format!(
            "failed to write managed runtime file: {}",
            display_path(path)
        )
    })?;
    report.created.push(path.to_path_buf());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_state(name: &str) -> StatePaths {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        StatePaths::from_root(std::env::temp_dir().join(format!(
            "vegasroom-paths-{name}-{}-{nanos}",
            std::process::id()
        )))
    }

    #[test]
    fn private_dirs_exclude_workspace() {
        let state = temp_state("private-dirs");

        assert!(state.private_dirs().contains(&state.root));
        assert!(state.private_dirs().contains(&state.pi_config));
        assert!(state.private_dirs().contains(&state.ssh_dir));
        assert!(!state.private_dirs().contains(&state.workspace));
    }

    #[cfg(unix)]
    #[test]
    fn ensure_applies_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let state = temp_state("private-permissions");
        state.ensure().unwrap();

        assert_eq!(
            fs::metadata(&state.root).unwrap().permissions().mode() & 0o777,
            PRIVATE_DIR_MODE
        );
        assert_eq!(
            fs::metadata(&state.config_yaml)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            PRIVATE_FILE_MODE
        );
        assert_eq!(
            fs::metadata(&state.runtime_compose_yaml)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            PRIVATE_FILE_MODE
        );

        let _ = fs::remove_dir_all(state.root);
    }
}
