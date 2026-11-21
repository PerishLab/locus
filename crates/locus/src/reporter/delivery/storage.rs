use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

pub(in crate::reporter) fn install(path: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let stage = dir.join(name);
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut temp = options.open(&stage)?;
    temp.write_all(bytes)?;
    temp.sync_all()?;
    drop(temp);
    fs::rename(stage, path)?;
    sync(dir)
}

pub(super) fn sync(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}
