use std::{fs, path::Path};

use super::{Check, Status};
use crate::{harness, paths::display_path};

#[cfg(unix)]
use crate::paths::{PRIVATE_DIR_MODE, PRIVATE_FILE_MODE};

pub(super) fn check_path_dir(name: &'static str, path: &Path) -> Check {
    if path.is_dir() {
        Check {
            status: Status::Pass,
            name,
            detail: format!("{} exists", display_path(path)),
        }
    } else if path.exists() {
        Check {
            status: Status::Fail,
            name,
            detail: format!(
                "expected directory path exists as a file: {}",
                display_path(path)
            ),
        }
    } else {
        Check {
            status: Status::Fail,
            name,
            detail: format!("missing directory: {}. Run: vr init", display_path(path)),
        }
    }
}

pub(super) fn check_path_file(name: &'static str, path: &Path) -> Check {
    if path.is_file() {
        Check {
            status: Status::Pass,
            name,
            detail: format!("{} exists", display_path(path)),
        }
    } else if path.exists() {
        Check {
            status: Status::Fail,
            name,
            detail: format!(
                "expected file path exists as a directory: {}",
                display_path(path)
            ),
        }
    } else {
        Check {
            status: Status::Fail,
            name,
            detail: format!("missing file: {}. Run: vr init", display_path(path)),
        }
    }
}

pub(super) fn check_known_hosts(path: &Path) -> Check {
    if path.is_file() {
        Check {
            status: Status::Pass,
            name: "known_hosts",
            detail: format!("{} exists", display_path(path)),
        }
    } else if path.exists() {
        Check {
            status: Status::Fail,
            name: "known_hosts",
            detail: format!(
                "expected known_hosts to be a file, but path exists as a directory: {}",
                display_path(path)
            ),
        }
    } else if path.parent().map(|parent| parent.is_dir()).unwrap_or(false) {
        Check {
            status: Status::Warn,
            name: "known_hosts",
            detail: format!(
                "{} is missing but can be created by `vr init`",
                display_path(path)
            ),
        }
    } else {
        Check {
            status: Status::Fail,
            name: "known_hosts",
            detail: format!("parent SSH directory is missing for {}", display_path(path)),
        }
    }
}

pub(super) fn check_dir_writable(name: &'static str, path: &Path) -> Check {
    if !path.is_dir() {
        return check_path_dir(name, path);
    }

    let test_file = path.join(".vr-doctor-write-test");
    match fs::write(&test_file, "doctor\n") {
        Ok(()) => {
            let _ = fs::remove_file(&test_file);
            Check {
                status: Status::Pass,
                name,
                detail: format!("{} is writable", display_path(path)),
            }
        }
        Err(err) => Check {
            status: Status::Fail,
            name,
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
        name: "Private directory permissions",
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
        name: "Private file permissions",
        detail: format!(
            "{} permission hardening check is skipped on non-Unix hosts",
            display_path(path)
        ),
    }
}

#[cfg(unix)]
fn check_private_permissions(name: &'static str, path: &Path, expected_mode: u32) -> Check {
    use std::os::unix::fs::PermissionsExt;

    if !path.exists() {
        return Check {
            status: Status::Warn,
            name,
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
                    name,
                    detail: format!(
                        "{} has private permissions {:03o}",
                        display_path(path),
                        expected_mode
                    ),
                }
            } else {
                Check {
                    status: Status::Warn,
                    name,
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
            name,
            detail: format!(
                "could not inspect permissions for {}: {err}",
                display_path(path)
            ),
        },
    }
}

pub(super) fn check_pi_auth_state(path: &Path) -> Check {
    check_harness_auth_state(path, &harness::PI)
}

fn check_harness_auth_state(path: &Path, descriptor: &harness::HarnessDescriptor) -> Check {
    let check_name = "Pi auth state";
    if path.is_file() {
        Check {
            status: Status::Pass,
            name: check_name,
            detail: format!("{} exists", display_path(path)),
        }
    } else if path.exists() {
        Check {
            status: Status::Fail,
            name: check_name,
            detail: format!(
                "expected {} auth state to be a file, but path exists as a directory: {}",
                descriptor.display_name,
                display_path(path)
            ),
        }
    } else {
        Check {
            status: Status::Warn,
            name: check_name,
            detail: format!(
                "{} not found. Run `cargo run -- {}`, then use {} `/login`.",
                display_path(path),
                descriptor.id,
                descriptor.display_name
            ),
        }
    }
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
}
