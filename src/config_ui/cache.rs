use std::{fs, path::PathBuf};

use anyhow::{Context, Result};

use crate::paths::{display_path, StatePaths};

pub(super) fn package_cache_paths(state: &StatePaths) -> Vec<PathBuf> {
    vec![
        state.cache.join("npm"),
        state.cache.join("pip"),
        state.cache.join("go-build"),
        state.cache.join("go-mod"),
        state.cargo_home.join("registry"),
        state.cargo_home.join("git"),
    ]
}

pub(super) fn purge_package_cache_paths(state: &StatePaths) -> Result<usize> {
    let mut purged = 0;
    for path in package_cache_paths(state) {
        if path.exists() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("failed to remove cache path: {}", display_path(&path)))?;
            purged += 1;
        }
    }
    Ok(purged)
}
