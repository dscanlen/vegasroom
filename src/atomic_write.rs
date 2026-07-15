use std::{
    fs::{self, File, OpenOptions, Permissions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn write_file(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    write_bytes(path, contents.as_ref())
}

pub fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    let contents = fs::read(source).with_context(|| {
        format!(
            "failed to read source file for atomic copy: {}",
            source.display()
        )
    })?;
    write_bytes(destination, &contents)
}

fn write_bytes(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = parent_dir(path);
    let permissions = existing_file_permissions(path)?;
    let (temp_path, mut temp_file) = create_temp_file(parent, path)?;

    let result = (|| {
        temp_file
            .write_all(contents)
            .with_context(|| format!("failed to write temporary file: {}", temp_path.display()))?;

        if let Some(permissions) = permissions {
            fs::set_permissions(&temp_path, permissions).with_context(|| {
                format!(
                    "failed to set temporary file permissions: {}",
                    temp_path.display()
                )
            })?;
        }

        temp_file
            .sync_all()
            .with_context(|| format!("failed to sync temporary file: {}", temp_path.display()))?;
        drop(temp_file);

        fs::rename(&temp_path, path).with_context(|| {
            format!(
                "failed to atomically replace {} with {}",
                path.display(),
                temp_path.display()
            )
        })?;
        sync_parent_dir(parent)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }

    result
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn existing_file_permissions(path: &Path) -> Result<Option<Permissions>> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(metadata.permissions())),
        Ok(_) => bail!("atomic write target is not a file: {}", path.display()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => {
            Err(err).with_context(|| format!("failed to read target metadata: {}", path.display()))
        }
    }
}

fn create_temp_file(parent: &Path, destination: &Path) -> Result<(PathBuf, File)> {
    let file_name = destination
        .file_name()
        .context("atomic write destination path does not have a file name")?
        .to_string_lossy();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    for _ in 0..1000 {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = parent.join(format!(
            ".{file_name}.tmp-{}-{nanos}-{counter}",
            std::process::id()
        ));

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        configure_private_create_mode(&mut options);

        match options.open(&temp_path) {
            Ok(file) => return Ok((temp_path, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("failed to create temporary file: {}", temp_path.display())
                });
            }
        }
    }

    bail!(
        "failed to allocate a temporary file for atomic write: {}",
        destination.display()
    )
}

#[cfg(unix)]
fn configure_private_create_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn configure_private_create_mode(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn sync_parent_dir(parent: &Path) -> Result<()> {
    let dir = File::open(parent)
        .with_context(|| format!("failed to open parent directory: {}", parent.display()))?;
    dir.sync_all()
        .with_context(|| format!("failed to sync parent directory: {}", parent.display()))
}

#[cfg(not(unix))]
fn sync_parent_dir(_parent: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_and_replaces_file() {
        let dir = unique_temp_dir("atomic-write");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.txt");

        write_file(&path, "one\n").unwrap();
        write_file(&path, "two\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "two\n");
        assert!(fs::read_dir(&dir).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp-")));

        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_existing_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = unique_temp_dir("atomic-write-permissions");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.txt");
        fs::write(&path, "old\n").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o640);
        fs::set_permissions(&path, permissions).unwrap();

        write_file(&path, "new\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new\n");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn atomic_copy_writes_destination_contents() {
        let dir = unique_temp_dir("atomic-copy");
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.txt");
        let destination = dir.join("destination.txt");
        fs::write(&source, "backup\n").unwrap();

        copy_file(&source, &destination).unwrap();

        assert_eq!(fs::read_to_string(&destination).unwrap(), "backup\n");

        let _ = fs::remove_dir_all(dir);
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("vegasroom-{name}-{}-{nanos}", std::process::id()))
    }
}
