//! Cooperating writers lock a stable sidecar; replacements never truncate live files.
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};

const LOCK_PREFIX: &str = ".keepbook-lock-";
const TEMP_PREFIX: &str = ".keepbook-tmp-";

#[cfg(feature = "git")]
pub(crate) fn is_internal_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(LOCK_PREFIX) || name.starts_with(TEMP_PREFIX))
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

fn with_writer_lock<R>(path: &Path, action: impl FnOnce() -> Result<R>) -> Result<R> {
    fs::create_dir_all(parent(path))?;
    let name = path.file_name().context("Storage path has no filename")?;
    let mut lock_name = std::ffi::OsString::from(LOCK_PREFIX);
    lock_name.push(name);
    lock_name.push(".lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(parent(path).join(lock_name))?;
    // Never unlink the sidecar: waiters must keep locking the same inode.
    lock.lock()?;
    action()
}

fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(parent(path))?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn replace_with(path: &Path, write: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    let permissions = match fs::metadata(path) {
        Ok(metadata) => {
            anyhow::ensure!(metadata.is_file(), "Not a regular file: {}", path.display());
            Some(metadata.permissions())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(err.into()),
    };
    let mut temporary = tempfile::Builder::new()
        .prefix(TEMP_PREFIX)
        .tempfile_in(parent(path))?;
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    write(temporary.as_file_mut())?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|err| err.error)?;
    sync_parent(path)
}

pub(crate) async fn write(path: &Path, content: Vec<u8>) -> Result<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        with_writer_lock(&path, || {
            replace_with(&path, |file| Ok(file.write_all(&content)?))
        })
        .with_context(|| format!("Failed to write {}", path.display()))
    })
    .await?
}

pub(crate) fn serialize_jsonl<T: Serialize>(items: &[T]) -> Result<Vec<u8>> {
    let mut content = Vec::new();
    for item in items {
        serde_json::to_writer(&mut content, item)?;
        content.push(b'\n');
    }
    Ok(content)
}

pub(crate) async fn append_jsonl<T: Serialize>(path: &Path, items: &[T]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let content = serialize_jsonl(items)?;
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        with_writer_lock(&path, || {
            let mut file = OpenOptions::new()
                .read(true)
                .append(true)
                .create(true)
                .open(&path)?;
            // Readers lock this file, so they cannot see a partially appended batch.
            file.lock()?;
            let original_len = file.metadata()?.len();
            let result = (|| -> Result<()> {
                if original_len > 0 {
                    file.seek(SeekFrom::End(-1))?;
                    let mut last = [0];
                    file.read_exact(&mut last)?;
                    if last[0] != b'\n' {
                        file.write_all(b"\n")?;
                    }
                }
                file.write_all(&content)?;
                file.sync_all()?;
                sync_parent(&path)
            })();
            if let Err(err) = result {
                file.set_len(original_len)
                    .and_then(|()| file.sync_all())
                    .with_context(|| format!("Append failed ({err:#}); rollback also failed"))?;
                return Err(err);
            }
            Ok(())
        })
        .with_context(|| format!("Failed to append {}", path.display()))
    })
    .await?
}

fn read_jsonl_sync<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };
    file.lock_shared()?;
    let mut items = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        if !line.trim().is_empty() {
            items.push(
                serde_json::from_str(&line).with_context(|| {
                    format!("Invalid JSONL at {}:{}", path.display(), index + 1)
                })?,
            );
        }
    }
    Ok(items)
}

pub(crate) async fn read_jsonl<T: DeserializeOwned + Send + 'static>(
    path: &Path,
) -> Result<Vec<T>> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        read_jsonl_sync(&path).with_context(|| format!("Failed to read {}", path.display()))
    })
    .await?
}

/// The closure returns its result and whether the file needs replacing.
/// Once started, the blocking transaction retains its lock even if its caller is cancelled.
pub(crate) async fn update_jsonl<T, R>(
    path: &Path,
    update: impl FnOnce(&mut Vec<T>) -> Result<(R, bool)> + Send + 'static,
) -> Result<R>
where
    T: DeserializeOwned + Serialize,
    R: Send + 'static,
{
    let path: PathBuf = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        with_writer_lock(&path, || {
            let mut items = read_jsonl_sync::<T>(&path)?;
            let (result, changed) = update(&mut items)?;
            if changed {
                let content = serialize_jsonl(&items)?;
                replace_with(&path, |file| Ok(file.write_all(&content)?))?;
            }
            Ok(result)
        })
        .with_context(|| format!("Failed to update {}", path.display()))
    })
    .await?
}

#[cfg(test)]
#[path = "../../tests/unit/storage/file_io_tests.rs"]
mod file_io_tests;
