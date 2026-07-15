use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};

use crate::{atomic_write, config::Config, paths::display_path};

pub(super) fn save_config_with_recovery_backup(config: &Config, config_path: &Path) -> Result<()> {
    let backup_path = if config_path.exists() {
        let backup_path = next_backup_path(config_path)?;
        atomic_write::copy_file(config_path, &backup_path).with_context(|| {
            format!(
                "failed to create recovery backup from {} to {}",
                display_path(config_path),
                display_path(&backup_path)
            )
        })?;
        Some(backup_path)
    } else {
        None
    };

    config.save_to_path(config_path)?;
    Config::load_from_path(config_path.to_path_buf())?;

    if let Some(backup_path) = backup_path {
        fs::remove_file(&backup_path).with_context(|| {
            format!(
                "saved config but failed to remove recovery backup: {}",
                display_path(&backup_path)
            )
        })?;
    }

    Ok(())
}

fn next_backup_path(config_path: &Path) -> Result<PathBuf> {
    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = config_path
        .file_name()
        .context("config path does not have a file name")?
        .to_string_lossy();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before Unix epoch")?
        .as_secs();

    for suffix in 0..1000 {
        let candidate = if suffix == 0 {
            parent.join(format!("{file_name}.backup-{timestamp}"))
        } else {
            parent.join(format!("{file_name}.backup-{timestamp}-{suffix}"))
        };

        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "could not allocate a backup path for config: {}",
        display_path(config_path)
    )
}
