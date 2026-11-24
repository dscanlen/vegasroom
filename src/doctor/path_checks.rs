use std::borrow::Cow;
use std::{fs, path::Path};

use super::{Check, Status};
use crate::{harness, paths::display_path};

#[cfg(unix)]
use crate::paths::{PRIVATE_DIR_MODE, PRIVATE_FILE_MODE};

pub(super) fn check_path_dir(name: impl Into<Cow<'static, str>>, path: &Path) -> Check {
    let name: Cow<'static, str> = name.into();
    if path.is_dir() {
        Check {
            status: Status::Pass,
            name: name.clone(),
            detail: format!("{} exists", display_path(path)),
        }
    } else if path.exists() {
        Check {
            status: Status::Fail,
            name: name.clone(),
            detail: format!(
                "expected directory path exists as a file: {}",
                display_path(path)
            ),
        }
    } else {
        Check {
            status: Status::Fail,
            name: name.clone(),
            detail: format!("missing directory: {}. Run: vr init", display_path(path)),
        }
    }
}

pub(super) fn check_path_file(name: impl Into<Cow<'static, str>>, path: &Path) -> Check {
    let name: Cow<'static, str> = name.into();
    if path.is_file() {
        Check {
            status: Status::Pass,
            name: name.clone(),
            detail: format!("{} exists", display_path(path)),
        }
    } else if path.exists() {
        Check {
            status: Status::Fail,
            name: name.clone(),
            detail: format!(
                "expected file path exists as a directory: {}",
                display_path(path)
            ),
        }
    } else {
        Check {
            status: Status::Fail,
            name: name.clone(),
            detail: format!("missing file: {}. Run: vr init", display_path(path)),
        }
    }
}

pub(super) fn check_known_hosts(path: &Path) -> Check {
    if path.is_file() {
        Check {
            status: Status::Pass,
            name: "known_hosts".into(),
            detail: format!("{} exists", display_path(path)),
        }
    } else if path.exists() {
        Check {
            status: Status::Fail,
            name: "known_hosts".into(),
            detail: format!(
                "expected known_hosts to be a file, but path exists as a directory: {}",
                display_path(path)
            ),
        }
    } else if path.parent().map(|parent| parent.is_dir()).unwrap_or(false) {
        Check {
            status: Status::Warn,
            name: "known_hosts".into(),
            detail: format!(
                "{} is missing but can be created by `vr init`",
                display_path(path)
            ),
        }
    } else {
        Check {
            status: Status::Fail,
            name: "known_hosts".into(),
            detail: format!("parent SSH directory is missing for {}", display_path(path)),
        }
    }
}

pub(super) fn check_dir_writable(name: impl Into<Cow<'static, str>>, path: &Path) -> Check {
    let name: Cow<'static, str> = name.into();
    if !path.is_dir() {
        return check_path_dir(name, path);
    }

    let test_file = path.join(".vr-doctor-write-test");
    match fs::write(&test_file, "doctor\n") {
        Ok(()) => {
            let _ = fs::remove_file(&test_file);
            Check {
                status: Status::Pass,
                name: name.clone(),
                detail: format!("{} is writable", display_path(path)),
            }
        }
        Err(err) => Check {
            status: Status::Fail,
            name: name.clone(),
            detail: format!("{} is not writable: {err}", display_path(path)),
        },
    }
}

#[cfg(unix)]
pub(super) fn check_private_dir_permissions(path: &Path) -> Check {
    check_private_permissions("Private directory permissions", path, PRIVATE_DIR_MODE)
}

#[cfg(not(unix))]
pub(super) fn check_private_dir_permissions(path: &Path) -> Check {
    Check {
        status: Status::Pass,
        name: "Private directory permissions".into(),
        detail: format!(
            "{} permission hardening check is skipped on non-Unix hosts",
            display_path(path)
        ),
    }
}

#[cfg(unix)]
pub(super) fn check_private_file_permissions(path: &Path) -> Check {
    check_private_permissions("Private file permissions", path, PRIVATE_FILE_MODE)
}

#[cfg(not(unix))]
pub(super) fn check_private_file_permissions(path: &Path) -> Check {
    Check {
        status: Status::Pass,
        name: "Private file permissions".into(),
        detail: format!(
            "{} permission hardening check is skipped on non-Unix hosts",
            display_path(path)
        ),
    }
}

#[cfg(unix)]
fn check_private_permissions(
    name: impl Into<Cow<'static, str>>,
    path: &Path,
    expected_mode: u32,
) -> Check {
    let name: Cow<'static, str> = name.into();
    use std::os::unix::fs::PermissionsExt;

    if !path.exists() {
        return Check {
            status: Status::Warn,
            name: name.clone(),
            detail: format!(
                "{} is missing; run `vr init` to recreate managed state",
                display_path(path)
            ),
        };
    }

    match fs::metadata(path) {
        Ok(metadata) => {
            let actual_mode = metadata.permissions().mode() & 0o777;
            if actual_mode == expected_mode {
                Check {
                    status: Status::Pass,
                    name: name.clone(),
                    detail: format!(
                        "{} has private permissions {:03o}",
                        display_path(path),
                        expected_mode
                    ),
                }
            } else {
                Check {
                    status: Status::Warn,
                    name: name.clone(),
                    detail: format!(
                        "{} has permissions {:03o}; expected {:03o}. Run: vr init",
                        display_path(path),
                        actual_mode,
                        expected_mode
                    ),
                }
            }
        }
        Err(err) => Check {
            status: Status::Warn,
            name: name.clone(),
            detail: format!(
                "could not inspect permissions for {}: {err}",
                display_path(path)
            ),
        },
    }
}

/// Login readiness for a harness: a persisted login file, or a host env var the
/// harness forwards into the room (e.g. ANTHROPIC_API_KEY for Claude Code).
pub(super) fn check_harness_auth_state(
    path: &Path,
    descriptor: &harness::HarnessDescriptor,
    host_env_is_set: impl Fn(&str) -> bool,
) -> Check {
    // Callers scope names per harness (e.g. "Claude Code: Auth state").
    let name: Cow<'static, str> = "Auth state".into();
    if path.is_file() {
        return Check {
            status: Status::Pass,
            name,
            detail: format!("{} exists", display_path(path)),
        };
    }

    if path.exists() {
        return Check {
            status: Status::Fail,
            name,
            detail: format!(
                "expected {} auth state to be a file, but path exists as a directory: {}",
                descriptor.display_name,
                display_path(path)
            ),
        };
    }

    let forwarded: Vec<&str> = descriptor
        .passthrough_env
        .iter()
        .copied()
        .filter(|key| host_env_is_set(key))
        .collect();
    if !forwarded.is_empty() {
        return Check {
            status: Status::Pass,
            name,
            detail: format!(
                "no persisted login at {}; {} will use {} forwarded from the host environment",
                display_path(path),
                descriptor.display_name,
                forwarded.join(", ")
            ),
        };
    }

    let env_hint = if descriptor.passthrough_env.is_empty() {
        String::new()
    } else {
        format!(
            ", or set {} in your shell",
            descriptor.passthrough_env.join(" or ")
        )
    };
    Check {
        status: Status::Warn,
        name,
        detail: format!(
            "{} not found. Run `vr {}` and log in (first-launch prompt, or `/login`){env_hint}.",
            display_path(path),
            descriptor.id,
        ),
    }
}

/// True when the host environment has a non-empty value for `key`.
pub(super) fn host_env_is_set(key: &str) -> bool {
    std::env::var_os(key).is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vegasroom-doctor-path-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[cfg(unix)]
    #[test]
    fn private_permission_check_warns_for_broad_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let path = temp_path("broad-dir");
        fs::create_dir_all(&path).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();

        let check = check_private_dir_permissions(&path);

        assert_eq!(check.status, Status::Warn);
        assert!(check.detail.contains("expected 700"));

        let _ = fs::remove_dir_all(path);
    }

    #[cfg(unix)]
    #[test]
    fn private_permission_check_passes_for_private_files() {
        use std::os::unix::fs::PermissionsExt;

        let path = temp_path("private-file");
        fs::write(&path, "test\n").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&path, permissions).unwrap();

        let check = check_private_file_permissions(&path);

        assert_eq!(check.status, Status::Pass);
        assert!(check.detail.contains("600"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn auth_state_passes_when_login_file_exists() {
        let root = temp_path("auth-file");
        fs::create_dir_all(&root).unwrap();
        let auth = root.join(".credentials.json");
        fs::write(&auth, "{}").unwrap();

        let check = check_harness_auth_state(&auth, &harness::CLAUDE, |_| false);
        assert_eq!(check.status, Status::Pass);
        assert_eq!(check.name, "Auth state");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn auth_state_passes_on_forwarded_host_env_without_login_file() {
        let missing = temp_path("auth-env").join(".credentials.json");

        let check = check_harness_auth_state(&missing, &harness::CLAUDE, |key| {
            key == "CLAUDE_CODE_OAUTH_TOKEN"
        });
        assert_eq!(check.status, Status::Pass);
        assert!(check.detail.contains("CLAUDE_CODE_OAUTH_TOKEN forwarded"));
        assert!(!check.detail.contains("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn auth_state_warns_with_login_and_env_hint_when_nothing_available() {
        let missing = temp_path("auth-none").join(".credentials.json");

        let check = check_harness_auth_state(&missing, &harness::CLAUDE, |_| false);
        assert_eq!(check.status, Status::Warn);
        assert!(check
            .detail
            .contains("Run `vr claude` and log in (first-launch prompt, or `/login`)"));
        assert!(check
            .detail
            .contains("ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN"));
        assert!(!check.detail.contains("cargo run"));
    }

    #[test]
    fn pi_auth_state_ignores_host_env_and_keeps_name() {
        let missing = temp_path("auth-pi").join("auth.json");

        let check = check_harness_auth_state(&missing, &harness::PI, |_| true);
        assert_eq!(check.status, Status::Warn);
        assert_eq!(check.name, "Auth state");
        assert!(check
            .detail
            .contains("Run `vr pi` and log in (first-launch prompt, or `/login`)."));
    }
}
