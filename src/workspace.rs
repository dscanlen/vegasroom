use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use directories::BaseDirs;

use crate::{
    config::Config,
    paths::{display_path, expand_tilde, StatePaths},
};

#[derive(Debug, Clone)]
pub struct ResolvedWorkspace {
    pub path: PathBuf,
    pub created: bool,
    pub warnings: Vec<String>,
}

impl ResolvedWorkspace {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn resolve_workspace(input: Option<&str>, config: &Config) -> Result<ResolvedWorkspace> {
    let state = StatePaths::default()?;
    let workspace_root = absolutize(expand_tilde(&config.paths.workspace))?;
    let input = input.map(str::trim).filter(|value| !value.is_empty());

    let request = match input {
        None => WorkspaceRequest {
            path: workspace_root.clone(),
            auto_create: can_auto_create_under_managed_workspace(
                &workspace_root,
                &workspace_root,
                &state,
            ),
        },
        Some(".") => WorkspaceRequest {
            path: env::current_dir().context("failed to resolve current directory")?,
            auto_create: false,
        },
        Some(value) if starts_with_tilde(value) => WorkspaceRequest {
            path: expand_tilde(value),
            auto_create: false,
        },
        Some(value) if Path::new(value).is_absolute() => WorkspaceRequest {
            path: PathBuf::from(value),
            auto_create: false,
        },
        Some(value) if contains_path_separator(value) => WorkspaceRequest {
            path: env::current_dir()
                .context("failed to resolve current directory")?
                .join(value),
            auto_create: false,
        },
        Some(value) => {
            reject_unsafe_workspace_name(value)?;
            let path = workspace_root.join(value);
            WorkspaceRequest {
                auto_create: can_auto_create_under_managed_workspace(
                    &path,
                    &workspace_root,
                    &state,
                ),
                path,
            }
        }
    };

    materialize_workspace(request, &state)
}

pub fn default_workspace_for_compose(config: &Config) -> Result<ResolvedWorkspace> {
    resolve_workspace(None, config)
}

fn materialize_workspace(
    request: WorkspaceRequest,
    state: &StatePaths,
) -> Result<ResolvedWorkspace> {
    let mut created = false;

    if request.path.exists() {
        if !request.path.is_dir() {
            bail!(
                "FAIL: Workspace path is not a directory: {}",
                display_path(&request.path)
            );
        }
    } else if request.auto_create {
        fs::create_dir_all(&request.path).with_context(|| {
            format!(
                "failed to create workspace directory: {}",
                display_path(&request.path)
            )
        })?;
        created = true;
    } else {
        bail!(
            "FAIL: Workspace path does not exist: {}\nCreate it first or choose an existing directory.",
            display_path(&request.path)
        );
    }

    let canonical = request.path.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize workspace path: {}",
            display_path(&request.path)
        )
    })?;
    let warnings = validate_workspace_path(&canonical, state)?;

    Ok(ResolvedWorkspace {
        path: canonical,
        created,
        warnings,
    })
}

fn validate_workspace_path(path: &Path, state: &StatePaths) -> Result<Vec<String>> {
    if path == Path::new("/") {
        bail!("FAIL: Refusing to mount / as a workspace.");
    }

    for blocked in blocked_virtual_roots() {
        if is_under_or_same(path, Path::new(blocked)) {
            bail!(
                "FAIL: Refusing to mount dangerous system path as a workspace: {}",
                path.display()
            );
        }
    }

    let mut warnings = Vec::new();

    if let Some(base_dirs) = BaseDirs::new() {
        let home = base_dirs
            .home_dir()
            .canonicalize()
            .unwrap_or_else(|_| base_dirs.home_dir().to_path_buf());

        if path == home {
            warnings.push(format!(
                "mounting the host home directory as /workspace exposes broad host files: {}",
                display_path(path)
            ));
        }

        for blocked in blocked_credential_roots(&home, state) {
            if is_under_or_same(path, &blocked) {
                bail!(
                    "FAIL: Refusing to mount credential directory as a workspace: {}",
                    display_path(path)
                );
            }
        }
    }

    for risky in risky_system_roots() {
        if is_under_or_same(path, Path::new(risky)) {
            warnings.push(format!(
                "mounting system path as /workspace may expose sensitive host files: {}",
                path.display()
            ));
            break;
        }
    }

    let state_root = state
        .root
        .canonicalize()
        .unwrap_or_else(|_| state.root.clone());
    let workspace_root = state
        .workspace
        .canonicalize()
        .unwrap_or_else(|_| state.workspace.clone());
    if is_under_or_same(path, &state_root) && !is_under_or_same(path, &workspace_root) {
        warnings.push(format!(
            "mounting Vegasroom state outside the managed workspace may expose auth or cache data: {}",
            display_path(path)
        ));
    }

    Ok(warnings)
}

fn blocked_credential_roots(home: &Path, state: &StatePaths) -> Vec<PathBuf> {
    let mut roots = vec![
        home.join(".ssh"),
        home.join(".config"),
        home.join(".aws"),
        home.join(".gcloud"),
        home.join(".kube"),
        home.join(".azure"),
        home.join(".docker"),
        home.join(".gnupg"),
        home.join(".password-store"),
        home.join(".local/share/keyrings"),
    ];

    roots.push(state.ssh_dir.clone());
    roots.push(state.pi_config.clone());
    roots
        .into_iter()
        .map(|path| path.canonicalize().unwrap_or(path))
        .collect()
}

fn blocked_virtual_roots() -> &'static [&'static str] {
    &["/dev", "/proc", "/sys", "/run"]
}

fn risky_system_roots() -> &'static [&'static str] {
    &[
        "/bin", "/boot", "/etc", "/lib", "/lib64", "/opt", "/sbin", "/tmp", "/usr", "/var",
    ]
}

fn reject_unsafe_workspace_name(name: &str) -> Result<()> {
    if name == "." || name == ".." {
        bail!("FAIL: Invalid workspace name: {name}");
    }

    if name.starts_with('-') {
        bail!(
            "FAIL: Workspace names cannot start with '-': {name}\nUse `vr pi -- {name}` to pass it to Pi, or use `./{name}` for a host path."
        );
    }

    Ok(())
}

fn can_auto_create_under_managed_workspace(
    target: &Path,
    workspace_root: &Path,
    state: &StatePaths,
) -> bool {
    let state_root = state
        .root
        .canonicalize()
        .unwrap_or_else(|_| state.root.clone());
    let workspace_root_abs =
        absolutize(workspace_root.to_path_buf()).unwrap_or_else(|_| workspace_root.to_path_buf());
    let target_abs = absolutize(target.to_path_buf()).unwrap_or_else(|_| target.to_path_buf());

    is_under_or_same(&workspace_root_abs, &state_root)
        && is_under_or_same(&target_abs, &workspace_root_abs)
}

fn absolutize(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(env::current_dir()
            .context("failed to resolve current directory")?
            .join(path))
    }
}

fn contains_path_separator(value: &str) -> bool {
    value.contains('/')
}

fn starts_with_tilde(value: &str) -> bool {
    value == "~" || value.starts_with("~/")
}

fn is_under_or_same(path: &Path, base: &Path) -> bool {
    path == base || path.starts_with(base)
}

struct WorkspaceRequest {
    path: PathBuf,
    auto_create: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = env::temp_dir().join(format!(
                "vegasroom-test-{name}-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn default_workspace_resolves_configured_existing_directory() {
        let root = TempDir::new("workspace-root");
        let mut config = Config::default();
        config.paths.workspace = root.path.display().to_string();

        let resolved = resolve_workspace(None, &config).unwrap();

        assert_eq!(resolved.path, root.path.canonicalize().unwrap());
        assert!(!resolved.created);
    }

    #[test]
    fn dot_workspace_resolves_current_directory() {
        let config = Config::default();
        let resolved = resolve_workspace(Some("."), &config).unwrap();

        assert_eq!(
            resolved.path,
            env::current_dir().unwrap().canonicalize().unwrap()
        );
        assert!(!resolved.created);
    }

    #[test]
    fn missing_absolute_workspace_fails_without_creating_it() {
        let root = TempDir::new("missing-absolute-parent");
        let missing = root.path.join("missing");
        let config = Config::default();

        let err = resolve_workspace(Some(&missing.display().to_string()), &config).unwrap_err();

        assert!(err.to_string().contains("Workspace path does not exist"));
        assert!(!missing.exists());
    }

    #[test]
    fn workspace_names_cannot_look_like_flags() {
        let config = Config::default();
        let err = resolve_workspace(Some("--session"), &config).unwrap_err();

        assert!(err
            .to_string()
            .contains("Workspace names cannot start with '-'"));
    }

    #[test]
    fn root_workspace_is_refused() {
        let state = StatePaths::default().unwrap();
        let err = validate_workspace_path(Path::new("/"), &state).unwrap_err();

        assert!(err.to_string().contains("Refusing to mount /"));
    }
}
