use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(super) fn spawn(
    path: &Path,
    directory: &Path,
    done: std::sync::mpsc::Sender<()>,
) -> std::io::Result<()> {
    let mut command = Command::new(path);
    command
        .env_clear()
        .current_dir(directory)
        .arg("deliver")
        .arg("--buffer")
        .arg(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for key in ["SSL_CERT_FILE", "SSL_CERT_DIR"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, 0x08000000);
    let mut child = command.spawn()?;
    std::thread::Builder::new()
        .name("locus-reap".into())
        .spawn(move || {
            let _ = child.wait();
            let _ = done.send(());
        })?;
    Ok(())
}

#[cfg(unix)]
pub(super) fn trusted(path: &Path) -> bool {
    if !path.is_absolute() || !path.is_file() {
        return false;
    }
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let uid = rustix::process::geteuid().as_raw();
        if !path.canonicalize().is_ok_and(|full| full == path) {
            return false;
        }
        for node in path.ancestors() {
            let Ok(meta) = std::fs::symlink_metadata(node) else {
                return false;
            };
            let mode = meta.permissions().mode();
            let sticky = meta.is_dir() && meta.uid() == 0 && mode & 0o1000 != 0;
            let writable = mode & 0o022 != 0 && !sticky;
            if (meta.uid() != 0 && meta.uid() != uid) || writable {
                return false;
            }
            if node == path && mode & 0o111 == 0 {
                return false;
            }
        }
    }
    true
}
#[cfg(not(unix))]
pub(super) fn trusted(path: &Path) -> bool {
    let _ = path;
    false
}

pub(super) fn installed() -> Option<PathBuf> {
    std::env::current_exe().ok().filter(|path| trusted(path))
}
