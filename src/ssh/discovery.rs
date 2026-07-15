use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{bail, Context, Result};

use crate::{
    alert,
    config::SelectedSshKey,
    paths::{display_path, expand_tilde},
};

use super::DiscoveredSshKey;

pub(crate) fn discovery_roots(paths: &[String]) -> Result<Vec<PathBuf>> {
    if paths.is_empty() {
        let home = directories::BaseDirs::new()
            .context("could not determine home directory")?
            .home_dir()
            .to_path_buf();
        return Ok(vec![home.join(".ssh")]);
    }

    Ok(paths.iter().map(|path| expand_tilde(path)).collect())
}

pub(crate) fn discover_keys(
    roots: &[PathBuf],
    follow_symlinks: bool,
) -> Result<Vec<DiscoveredSshKey>> {
    let mut keys = Vec::new();
    let mut visited_dirs = HashSet::new();

    for root in roots {
        if !root.exists() {
            println!(
                "{}: scan root does not exist: {}",
                alert::warn(),
                display_path(root)
            );
            continue;
        }
        scan_path(root, follow_symlinks, &mut visited_dirs, &mut keys)?;
    }

    keys.dedup_by(|a, b| a.path == b.path);
    Ok(keys)
}

pub(super) fn scan_path(
    path: &Path,
    follow_symlinks: bool,
    visited_dirs: &mut HashSet<PathBuf>,
    keys: &mut Vec<DiscoveredSshKey>,
) -> Result<()> {
    let metadata = fs::symlink_metadata(path).with_context(|| {
        format!(
            "failed to inspect path while scanning SSH keys: {}",
            display_path(path)
        )
    })?;

    if metadata.file_type().is_symlink() && !follow_symlinks {
        return Ok(());
    }

    let metadata = if follow_symlinks {
        fs::metadata(path)?
    } else {
        metadata
    };

    if metadata.is_dir() {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !visited_dirs.insert(canonical) {
            return Ok(());
        }
        for entry in fs::read_dir(path).with_context(|| {
            format!(
                "failed to read directory while scanning SSH keys: {}",
                display_path(path)
            )
        })? {
            let entry = entry?;
            scan_path(&entry.path(), follow_symlinks, visited_dirs, keys)?;
        }
        return Ok(());
    }

    if metadata.is_file() && is_private_key_candidate(path) {
        if let Ok(key) = inspect_private_key(path) {
            keys.push(key);
        }
    }

    Ok(())
}

pub(super) fn is_private_key_candidate(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    if name.ends_with(".pub")
        || matches!(
            name,
            "known_hosts" | "known_hosts.old" | "authorized_keys" | "config"
        )
        || name.ends_with(".bak")
        || name.ends_with(".old")
        || name.ends_with(".tmp")
    {
        return false;
    }

    let Ok(contents) = fs::read(path) else {
        return false;
    };
    let sample_len = contents.len().min(4096);
    let sample = String::from_utf8_lossy(&contents[..sample_len]);
    sample.contains("PRIVATE KEY")
}

pub(super) fn inspect_private_key(path: &Path) -> Result<DiscoveredSshKey> {
    let metadata = fingerprint_key(path).unwrap_or_default();
    Ok(DiscoveredSshKey {
        path: path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
        display_path: display_path(path),
        fingerprint: metadata.fingerprint,
        comment: metadata.comment,
        key_type: metadata.key_type,
        has_public_pair: path
            .with_extension(format!(
                "{}pub",
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| format!("{ext}."))
                    .unwrap_or_default()
            ))
            .is_file()
            || PathBuf::from(format!("{}.pub", path.display())).is_file(),
        permissions_ok: private_key_permissions_ok(path),
    })
}

#[derive(Debug, Default)]
pub(super) struct KeyMetadata {
    pub(super) fingerprint: Option<String>,
    pub(super) comment: Option<String>,
    pub(super) key_type: Option<String>,
}

pub(super) fn fingerprint_key(path: &Path) -> Result<KeyMetadata> {
    let output = Command::new("ssh-keygen")
        .arg("-lf")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run ssh-keygen for {}", display_path(path)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!(
            "ssh-keygen could not fingerprint {}: {stderr}",
            display_path(path)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ssh_keygen_fingerprint(stdout.trim()))
}

pub(super) fn parse_ssh_keygen_fingerprint(line: &str) -> KeyMetadata {
    let mut metadata = KeyMetadata::default();
    let mut parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() >= 2 {
        metadata.fingerprint = Some(parts[1].to_owned());
    }

    if let Some(last) = parts.last() {
        if last.starts_with('(') && last.ends_with(')') {
            metadata.key_type = Some(
                last.trim_start_matches('(')
                    .trim_end_matches(')')
                    .to_owned(),
            );
            parts.pop();
        }
    }

    if parts.len() > 2 {
        metadata.comment = Some(parts[2..].join(" "));
    }

    metadata
}

pub(crate) fn initial_selection(
    discovered: &[DiscoveredSshKey],
    selected: &[SelectedSshKey],
) -> Vec<bool> {
    discovered
        .iter()
        .map(|candidate| {
            selected.iter().any(|configured| {
                let configured_path = expand_tilde(&configured.path);
                configured_path == candidate.path
                    || configured.fingerprint.is_some()
                        && configured.fingerprint == candidate.fingerprint
            })
        })
        .collect()
}

#[cfg(unix)]
fn private_key_permissions_ok(path: &Path) -> Option<bool> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path).ok()?.permissions().mode();
    Some(mode & 0o077 == 0)
}

#[cfg(not(unix))]
fn private_key_permissions_ok(_path: &Path) -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_keygen_fingerprint_with_comment_and_type() {
        let metadata =
            parse_ssh_keygen_fingerprint("256 SHA256:abc123 user@example.com test key (ED25519)");

        assert_eq!(metadata.fingerprint.as_deref(), Some("SHA256:abc123"));
        assert_eq!(
            metadata.comment.as_deref(),
            Some("user@example.com test key")
        );
        assert_eq!(metadata.key_type.as_deref(), Some("ED25519"));
    }

    #[test]
    fn initial_selection_matches_by_fingerprint() {
        let discovered = vec![DiscoveredSshKey {
            path: PathBuf::from("/tmp/current-key"),
            display_path: "/tmp/current-key".to_owned(),
            fingerprint: Some("SHA256:abc123".to_owned()),
            comment: None,
            key_type: Some("ED25519".to_owned()),
            has_public_pair: false,
            permissions_ok: None,
        }];
        let selected = vec![SelectedSshKey {
            path: "/old/path".to_owned(),
            fingerprint: Some("SHA256:abc123".to_owned()),
            comment: None,
            key_type: None,
            git_user_name: None,
            git_user_email: None,
        }];

        assert_eq!(initial_selection(&discovered, &selected), vec![true]);
    }
}
