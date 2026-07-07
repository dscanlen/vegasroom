use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};

use crate::paths::{display_path, StatePaths};

static RUNTIME_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(super) struct RuntimeFiles {
    dir: PathBuf,
}

impl RuntimeFiles {
    pub(super) fn new(state: &StatePaths) -> Result<Self> {
        fs::create_dir_all(&state.cache).with_context(|| {
            format!(
                "failed to create cache directory: {}",
                display_path(&state.cache)
            )
        })?;

        for _ in 0..10 {
            let dir = unique_runtime_dir(&state.cache);
            match fs::create_dir(&dir) {
                Ok(()) => return Ok(Self { dir }),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!(
                            "failed to create per-launch runtime directory: {}",
                            display_path(&dir)
                        )
                    });
                }
            }
        }

        Err(anyhow!(
            "failed to allocate a unique per-launch runtime directory under {}",
            display_path(&state.cache)
        ))
    }

    pub(super) fn dir(&self) -> &Path {
        &self.dir
    }
}

impl Drop for RuntimeFiles {
    fn drop(&mut self) {
        if self
            .dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("run-"))
            .unwrap_or(false)
        {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

fn unique_runtime_dir(cache: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = RUNTIME_COUNTER.fetch_add(1, Ordering::Relaxed);
    cache.join(format!("run-{}-{nanos}-{counter}", std::process::id()))
}
