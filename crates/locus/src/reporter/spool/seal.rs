use super::{ACTIVE, AGE, LOCK, Root, count, counted};
use radicula_lock::{Error, Factory, Failure, Policy, Wait};
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
    let gate = Factory::file(lock, Policy::EXCLUSIVE).map_err(io::Error::other)?;
    match gate.with(Wait::immediate(), || rotate(&root, gate.file(), now)) {
        Ok(output) => output,
        Err(Failure::Admission(Error::Busy)) => Ok(()),
        Err(Failure::Admission(Error::Io(error))) => Err(io::Error::new(error.kind(), error)),
        Err(Failure::Admission(error)) => Err(io::Error::other(error)),
        Err(Failure::Release { output, error }) => {
            let message = format!("cannot unlock spool: {error}");
            Err(match output {
                Ok(()) => io::Error::other(message),
                Err(business) => io::Error::new(business.kind(), format!("{business}; {message}")),
            })
        }
    }
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
