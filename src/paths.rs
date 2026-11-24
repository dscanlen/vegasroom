use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use directories::BaseDirs;

use crate::{
    assets, atomic_write,
    config::DEFAULT_CONFIG_YAML,
    harness::{self, HarnessDescriptor},
};

#[derive(Debug, Clone)]
pub struct StatePaths {
    pub root: PathBuf,
    pub harness_root: PathBuf,
    pub workspace: PathBuf,
    pub environment_root: PathBuf,
    pub cargo_home: PathBuf,
    pub runtime_root: PathBuf,
    pub runtime_harness_root: PathBuf,
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

/// Runtime dir name for the shared base image; reserved, so no harness may use it as an id.
pub const BASE_RUNTIME_DIR: &str = "base";

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
        let workspace = root.join("workspace");
        let environment_root = root.join("environment");
        let cargo_home = environment_root.join("cargo");
        let runtime_root = root.join("runtime");
        let runtime_harness_root = runtime_root.join("harness");
        let runtime_compose_yaml = runtime_root.join("compose.yaml");
        let ssh_dir = root.join("ssh");
        let known_hosts = ssh_dir.join("known_hosts");
        let cache = root.join("cache");
        let config_yaml = root.join("config.yaml");
        let disclaimer_ack = cache.join("disclaimer-ack");

        Self {
            root,
            harness_root,
            workspace,
            environment_root,
            cargo_home,
            runtime_root,
            runtime_harness_root,
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
        for descriptor in harness::HARNESSES {
            let Some(contents) = assets::managed_dockerfile(descriptor) else {
                bail!("no embedded Dockerfile for harness `{}`", descriptor.id);
            };
            write_managed_file(&self.runtime_dockerfile(descriptor), contents, &mut report)?;
        }
        self.apply_private_permissions()?;

        Ok(report)
    }

    /// `~/.vegasroom/harness/<id>`
    pub fn harness_state_root(&self, descriptor: &HarnessDescriptor) -> PathBuf {
        self.harness_root.join(descriptor.id)
    }

    /// `~/.vegasroom/harness/<id>/<name>`
    pub fn harness_state_dir(&self, descriptor: &HarnessDescriptor, name: &str) -> PathBuf {
        self.harness_state_root(descriptor).join(name)
    }

    /// Host paths of every state dir the descriptor mounts, in descriptor order.
    pub fn harness_state_dirs(&self, descriptor: &HarnessDescriptor) -> Vec<PathBuf> {
        descriptor
            .state_dirs
            .iter()
            .map(|dir| self.harness_state_dir(descriptor, dir.name))
            .collect()
    }

    /// Host path of the harness's persisted login/auth file.
    pub fn harness_auth_path(&self, descriptor: &HarnessDescriptor) -> PathBuf {
        self.harness_state_root(descriptor)
            .join(descriptor.auth_state_relative_path)
    }

    /// Directory holding the harness's persisted login/auth file (treated as a credential root).
    pub fn harness_auth_dir(&self, descriptor: &HarnessDescriptor) -> PathBuf {
        let auth = self.harness_auth_path(descriptor);
        auth.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.harness_state_root(descriptor))
    }

    /// `~/.vegasroom/runtime/harness/<id>`
    pub fn runtime_harness_dir(&self, descriptor: &HarnessDescriptor) -> PathBuf {
        self.runtime_harness_root.join(descriptor.id)
    }

    /// `~/.vegasroom/runtime/harness/base` — build context for the shared base image.
    pub fn runtime_base_dir(&self) -> PathBuf {
        self.runtime_harness_root.join(BASE_RUNTIME_DIR)
    }

    /// Generated Dockerfile for the shared base image (foundation + environment profile).
    pub fn runtime_base_dockerfile(&self) -> PathBuf {
        self.runtime_base_dir().join("Dockerfile")
    }

    /// Managed Dockerfile location for a harness, relative to the compose project dir.
    pub fn runtime_dockerfile(&self, descriptor: &HarnessDescriptor) -> PathBuf {
        self.runtime_root.join(descriptor.dockerfile_path)
    }

    /// Per-harness state dirs (root + each mounted dir) and runtime dirs for every registered harness.
    fn harness_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.harness_root.clone()];
        for descriptor in harness::HARNESSES {
            dirs.push(self.harness_state_root(descriptor));
            dirs.extend(self.harness_state_dirs(descriptor));
        }
        dirs
    }

    fn runtime_harness_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.runtime_harness_root.clone(), self.runtime_base_dir()];
        dirs.extend(
            harness::HARNESSES
                .iter()
                .map(|descriptor| self.runtime_harness_dir(descriptor)),
        );
        dirs
    }

    pub fn required_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.root.clone()];
        dirs.extend(self.harness_dirs());
        dirs.extend([
            self.workspace.clone(),
            self.environment_root.clone(),
            self.cargo_home.clone(),
            self.runtime_root.clone(),
        ]);
        dirs.extend(self.runtime_harness_dirs());
        dirs.extend([self.ssh_dir.clone(), self.cache.clone()]);
        dirs
    }

    pub fn private_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.root.clone()];
        dirs.extend(self.harness_dirs());
        dirs.extend([
            self.environment_root.clone(),
            self.cargo_home.clone(),
            self.runtime_root.clone(),
        ]);
        dirs.extend(self.runtime_harness_dirs());
        dirs.extend([self.ssh_dir.clone(), self.cache.clone()]);
        dirs
    }

    pub fn private_files(&self) -> Vec<PathBuf> {
        let mut files = vec![
            self.config_yaml.clone(),
            self.known_hosts.clone(),
            self.runtime_compose_yaml.clone(),
            self.runtime_base_dockerfile(),
        ];
        files.extend(
            harness::HARNESSES
                .iter()
                .map(|descriptor| self.runtime_dockerfile(descriptor)),
        );
        files
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
        println!("Provider login state persists inside each harness's state mount (~/.vegasroom/harness/<id>) after you log in from the room (Pi /login, or Claude Code's first-launch browser login).\n");
        println!("Claude Code also receives ANTHROPIC_API_KEY and CLAUDE_CODE_OAUTH_TOKEN from your shell when they are set.\n");
        let names: Vec<&str> = harness::HARNESSES.iter().map(|d| d.display_name).collect();
        println!(
            "Available harnesses: {}. Default harness: {}.\n",
            names.join(", "),
            harness::HARNESSES[0].display_name
        );

        atomic_write::write_file(&self.disclaimer_ack, "acknowledged\n").with_context(|| {
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

    atomic_write::write_file(path, contents)
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

    atomic_write::write_file(path, contents).with_context(|| {
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
        assert!(state
            .private_dirs()
            .contains(&state.harness_state_dir(&harness::PI, harness::PI_CONFIG_DIR)));
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

        // runtime_compose_yaml is written by `vr init` (docker::write_runtime_compose_yaml),
        // not by ensure(). Verify that apply_private_permissions covers it once it exists.
        fs::write(&state.runtime_compose_yaml, "test\n").unwrap();
        state.apply_private_permissions().unwrap();
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

    #[test]
    fn harness_helpers_resolve_expected_pi_layout() {
        let state = temp_state("pi-layout");
        let pi = &harness::PI;
        let pi_root = state.root.join("harness/pi");

        assert_eq!(state.harness_state_root(pi), pi_root);
        assert_eq!(
            state.harness_state_dir(pi, harness::PI_CONFIG_DIR),
            pi_root.join("config")
        );
        assert_eq!(
            state.harness_state_dir(pi, harness::PI_SESSIONS_DIR),
            pi_root.join("sessions")
        );
        assert_eq!(
            state.harness_auth_path(pi),
            pi_root.join("config/auth.json")
        );
        assert_eq!(state.harness_auth_dir(pi), pi_root.join("config"));
        assert_eq!(
            state.runtime_dockerfile(pi),
            state.runtime_root.join("harness/pi/Dockerfile")
        );
    }

    #[test]
    fn base_runtime_paths_do_not_collide_with_harness_ids() {
        let state = temp_state("base-layout");
        assert_eq!(
            state.runtime_base_dockerfile(),
            state.runtime_root.join("harness/base/Dockerfile")
        );
        for descriptor in harness::HARNESSES {
            assert_ne!(descriptor.id, BASE_RUNTIME_DIR);
        }
        assert!(state.required_dirs().contains(&state.runtime_base_dir()));
        assert!(state
            .private_files()
            .contains(&state.runtime_base_dockerfile()));
    }

    #[test]
    fn harness_helpers_resolve_expected_claude_layout() {
        let state = temp_state("claude-layout");
        let claude = &harness::CLAUDE;
        let claude_root = state.root.join("harness/claude");

        assert_eq!(
            state.harness_state_dirs(claude),
            vec![claude_root.join("config")]
        );
        assert_eq!(
            state.harness_auth_path(claude),
            claude_root.join("config/.credentials.json")
        );
        assert_eq!(state.harness_auth_dir(claude), claude_root.join("config"));
        assert_eq!(
            state.runtime_dockerfile(claude),
            state.runtime_root.join("harness/claude/Dockerfile")
        );
    }

    #[test]
    fn required_and_private_dirs_cover_every_registered_harness() {
        let state = temp_state("harness-dirs");
        let required = state.required_dirs();
        let private = state.private_dirs();
        let files = state.private_files();

        for descriptor in harness::HARNESSES {
            for dir in state.harness_state_dirs(descriptor) {
                assert!(
                    required.contains(&dir),
                    "required missing {}",
                    dir.display()
                );
                assert!(private.contains(&dir), "private missing {}", dir.display());
            }
            assert!(required.contains(&state.runtime_harness_dir(descriptor)));
            assert!(files.contains(&state.runtime_dockerfile(descriptor)));
        }
    }

    #[test]
    fn ensure_writes_managed_dockerfile_for_every_registered_harness() {
        let state = temp_state("managed-dockerfiles");
        state.ensure().unwrap();

        for descriptor in harness::HARNESSES {
            let written = fs::read_to_string(state.runtime_dockerfile(descriptor)).unwrap();
            assert_eq!(
                Some(written.as_str()),
                assets::managed_dockerfile(descriptor)
            );
            for dir in state.harness_state_dirs(descriptor) {
                assert!(dir.is_dir(), "missing {}", dir.display());
            }
        }

        let _ = fs::remove_dir_all(state.root);
    }
}
