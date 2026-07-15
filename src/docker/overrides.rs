use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::{atomic_write, config::Config, harness, paths::display_path};

pub(super) fn prepare_read_only_rootfs(
    config: &Config,
    runtime_dir: &Path,
) -> Result<Option<PathBuf>> {
    if !config.harness.pi.read_only_rootfs {
        return Ok(None);
    }

    fs::create_dir_all(runtime_dir).with_context(|| {
        format!(
            "failed to create per-launch runtime directory: {}",
            display_path(runtime_dir)
        )
    })?;

    let override_path = runtime_dir.join("read-only-rootfs.compose.yaml");
    let contents = format!(
        r#"services:
  {service_name}:
    read_only: true
    tmpfs:
      - /tmp
      - /run
      - /var/tmp
"#,
        service_name = harness::PI.service_name,
    );

    atomic_write::write_file(&override_path, contents).with_context(|| {
        format!(
            "failed to write read-only rootfs Compose override: {}",
            display_path(&override_path)
        )
    })?;

    Ok(Some(override_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_state_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("vegasroom-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn read_only_rootfs_override_is_only_written_when_enabled() {
        let root = test_state_root("read-only-rootfs");
        fs::create_dir_all(&root).unwrap();
        let mut config = Config::default();

        assert!(prepare_read_only_rootfs(&config, &root).unwrap().is_none());

        config.harness.pi.read_only_rootfs = true;
        let override_path = prepare_read_only_rootfs(&config, &root).unwrap().unwrap();
        let contents = fs::read_to_string(&override_path).unwrap();

        assert!(contents.contains("  pi:"));
        assert!(contents.contains("read_only: true"));
        assert!(contents.contains("- /tmp"));
        assert!(contents.contains("- /run"));
        assert!(contents.contains("- /var/tmp"));

        let _ = fs::remove_dir_all(root);
    }
}
