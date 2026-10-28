use super::{ACTIVE, AGE, LOCK, Root, count, counted};
use radicula_lock::{Mode, TryLockError};
use std::fs::{self, File, Metadata};
use std::io;
use std::path::Path;
use std::time::SystemTime;

pub(super) fn expired(directory: &Path, now: SystemTime) -> io::Result<()> {
    if !stale(directory, now)? {
        return Ok(());
    }
    let root = Root(directory.to_path_buf());
    let lock = root.open(LOCK, false).map_err(io::Error::other)?;
    match radicula_lock::attempt(&lock, Mode::Exclusive) {
        Ok(()) => (),
        Err(TryLockError::WouldBlock) => return Ok(()),
        Err(TryLockError::Error(error)) => return Err(error),
    }
    let result = rotate(&root, &lock, now);
    let released = radicula_lock::unlock(&lock);
    result.and(released)
}

fn metadata(directory: &Path) -> io::Result<Option<Metadata>> {
    match fs::metadata(directory.join(ACTIVE)) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn stale(directory: &Path, now: SystemTime) -> io::Result<bool> {
    let Some(metadata) = metadata(directory)? else {
        return Ok(false);
    };
    if metadata.len() == 0 {
        return Ok(false);
    }
    let since = metadata.created().or_else(|_| metadata.modified())?;
    Ok(now.duration_since(since).is_ok_and(|age| age >= AGE))
}

fn rotate(root: &Root, lock: &File, now: SystemTime) -> io::Result<()> {
    if !stale(&root.0, now)? {
        return Ok(());
    }
    let size = fs::metadata(root.0.join(ACTIVE))?.len();
    let sealed = match counted(lock).map_err(io::Error::other)? {
        Some(bytes) => bytes,
        None => root.total().map_err(io::Error::other)?,
    };
    count(lock, sealed.saturating_add(size)).map_err(io::Error::other)?;
    fs::rename(root.0.join(ACTIVE), root.stamped())
}
