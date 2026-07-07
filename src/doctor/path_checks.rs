use std::{fs, path::Path};

use super::{Check, Status};
use crate::{harness, paths::display_path};

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
