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

pub(super) struct PackageCacheEstimate {
    pub(super) path: PathBuf,
    pub(super) bytes: u64,
}

pub(super) fn package_cache_estimates(state: &StatePaths) -> Vec<PackageCacheEstimate> {
    package_cache_paths(state)
        .into_iter()
        .map(|path| PackageCacheEstimate {
            bytes: directory_size(&path).unwrap_or(0),
            path,
        })
        .collect()
}

pub(super) fn total_package_cache_bytes(estimates: &[PackageCacheEstimate]) -> u64 {
    estimates.iter().map(|estimate| estimate.bytes).sum()
}

fn directory_size(path: &PathBuf) -> Result<u64> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to read cache path metadata: {}", display_path(path))
            })
        }
    };

    if metadata.is_file() {
        return Ok(metadata.len());
    }

    if !metadata.is_dir() {
        return Ok(0);
    }

    let mut total = 0;
    for entry in fs::read_dir(path)
        .with_context(|| format!("failed to read cache path: {}", display_path(path)))?
    {
        let entry = entry
            .with_context(|| format!("failed to read cache path entry: {}", display_path(path)))?;
        total += directory_size(&entry.path())?;
    }
    Ok(total)
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
