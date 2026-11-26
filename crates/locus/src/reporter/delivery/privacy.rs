use std::path::Path;

#[cfg(unix)]
const ENTRIES: &[&str] = &[
    "delivery.json",
    "queue.db",
    "queue.db-wal",
    "queue.db-shm",
    "spool.lock",
    "sender.lock",
];

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    uid: u32,
    gid: u32,
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stat {
    uid: u32,
    gid: u32,
    mode: u32,
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Ready,
    Tighten,
    Refuse,
}

#[cfg(unix)]
fn bits(mode: impl Into<u64>) -> u32 {
    (mode.into() & 0o7777) as u32
}

#[cfg(unix)]
fn judge(stat: Stat, owner: Identity, heal: bool) -> Verdict {
    if stat.uid != owner.uid {
        return Verdict::Refuse;
    }
    if stat.mode & 0o077 == 0 {
        return Verdict::Ready;
    }
    if heal && stat.gid == owner.gid && stat.mode & 0o007 == 0 {
        return Verdict::Tighten;
    }
    Verdict::Refuse
}

#[cfg(unix)]
pub(in crate::reporter) fn verify(directory: &Path) -> Result<(), String> {
    Dir(directory).verify()
}

#[cfg(unix)]
pub(in crate::reporter) fn tighten(directory: &Path) -> Result<(), String> {
    Dir(directory).tighten()
}

#[cfg(not(unix))]
pub(in crate::reporter) fn verify(_: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(not(unix))]
pub(in crate::reporter) fn tighten(_: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
struct Dir<'a>(&'a Path);

#[cfg(unix)]
impl Dir<'_> {
    fn verify(&self) -> Result<(), String> {
        let fd = self.open()?;
        let stat = self.inspect(&fd)?;
        let owner = current();
        match judge(stat, owner, false) {
            Verdict::Ready => Ok(()),
            Verdict::Tighten | Verdict::Refuse => Err(self.refusal(stat, owner)),
        }
    }

    fn tighten(&self) -> Result<(), String> {
        let fd = self.open()?;
        let stat = self.inspect(&fd)?;
        let owner = current();
        match judge(stat, owner, true) {
            Verdict::Ready => Ok(()),
            Verdict::Refuse => Err(self.refusal(stat, owner)),
            Verdict::Tighten => {
                let entries = self.files(&fd, owner.uid)?;
                self.apply(&fd, &entries)
            }
        }
    }

    fn open(&self) -> Result<rustix::fd::OwnedFd, String> {
        use rustix::fs::{Mode, OFlags};
        use rustix::io::Errno;
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW;
        rustix::fs::open(self.0, flags, Mode::empty()).map_err(|error| {
            if error == Errno::LOOP || error == Errno::NOTDIR {
                format!(
                    "HTTPS buffer directory '{}' must be a private directory (not a symlink or file): {error}",
                    self.0.display()
                )
            } else {
                format!(
                    "cannot inspect HTTPS buffer directory '{}': {error}",
                    self.0.display()
                )
            }
        })
    }

    fn inspect(&self, fd: &rustix::fd::OwnedFd) -> Result<Stat, String> {
        let stat = rustix::fs::fstat(fd).map_err(|error| {
            format!(
                "cannot inspect HTTPS buffer directory '{}': {error}",
                self.0.display()
            )
        })?;
        Ok(Stat {
            uid: stat.st_uid,
            gid: stat.st_gid,
            mode: bits(stat.st_mode),
        })
    }

    fn files(
        &self,
        fd: &rustix::fd::OwnedFd,
        owner: u32,
    ) -> Result<Vec<rustix::fd::OwnedFd>, String> {
        let mut opened = Vec::new();
        for name in ENTRIES {
            if let Some(file) = self.check(fd, name, owner)? {
                opened.push(file);
            }
        }
        Ok(opened)
    }

    fn check(
        &self,
        fd: &rustix::fd::OwnedFd,
        name: &str,
        owner: u32,
    ) -> Result<Option<rustix::fd::OwnedFd>, String> {
        use rustix::fs::{FileType, Mode, OFlags};
        use rustix::io::Errno;
        let path = self.0.join(name);
        let file =
            match rustix::fs::openat(fd, name, OFlags::RDONLY | OFlags::NOFOLLOW, Mode::empty()) {
                Ok(file) => file,
                Err(Errno::NOENT) => return Ok(None),
                Err(Errno::LOOP) => {
                    return Err(format!(
                        "HTTPS buffer entry '{}' must not be a symlink",
                        path.display()
                    ));
                }
                Err(error) => {
                    return Err(format!(
                        "cannot inspect HTTPS buffer entry '{}': {error}",
                        path.display()
                    ));
                }
            };
        let stat = rustix::fs::fstat(&file).map_err(|error| {
            format!(
                "cannot inspect HTTPS buffer entry '{}': {error}",
                path.display()
            )
        })?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
            return Err(format!(
                "HTTPS buffer entry '{}' must be a regular file",
                path.display()
            ));
        }
        if stat.st_uid != owner {
            return Err(format!(
                "HTTPS buffer entry '{}' must be owned by uid {owner}: found owner {}:{}",
                path.display(),
                stat.st_uid,
                stat.st_gid
            ));
        }
        Ok(Some(file))
    }

    fn apply(
        &self,
        fd: &rustix::fd::OwnedFd,
        entries: &[rustix::fd::OwnedFd],
    ) -> Result<(), String> {
        use rustix::fs::Mode;
        rustix::fs::fchmod(fd, Mode::from_raw_mode(0o700)).map_err(|error| {
            format!(
                "cannot tighten HTTPS buffer directory '{}' to mode 0700: {error}",
                self.0.display()
            )
        })?;
        for file in entries {
            rustix::fs::fchmod(file, Mode::from_raw_mode(0o600)).map_err(|error| {
                format!(
                    "cannot tighten HTTPS buffer file inside '{}' to mode 0600: {error}",
                    self.0.display()
                )
            })?;
        }
        Ok(())
    }

    fn refusal(&self, stat: Stat, owner: Identity) -> String {
        format!(
            "HTTPS buffer directory '{}' must be private (mode 0700, owned by uid {}): found mode {:04o}, owner {}:{} (process {}:{}); mount volumes at a parent directory, point buffer to a subdirectory, and set fsGroupChangePolicy: OnRootMismatch",
            self.0.display(),
            owner.uid,
            stat.mode,
            stat.uid,
            stat.gid,
            owner.uid,
            owner.gid
        )
    }
}

#[cfg(unix)]
fn current() -> Identity {
    Identity {
        uid: rustix::process::geteuid().as_raw(),
        gid: rustix::process::getegid().as_raw(),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sample(uid: u32, gid: u32, mode: u32) -> Stat {
        Stat { uid, gid, mode }
    }

    #[test]
    fn verdicts() {
        let owner = Identity {
            uid: 10001,
            gid: 10001,
        };
        let cases = [
            (sample(10001, 10001, 0o0700), false, Verdict::Ready),
            (sample(10001, 10001, 0o2700), false, Verdict::Ready),
            (sample(10001, 10001, 0o2770), true, Verdict::Tighten),
            (sample(10001, 10001, 0o2770), false, Verdict::Refuse),
            (sample(0, 10001, 0o0700), true, Verdict::Refuse),
            (sample(10001, 20002, 0o2770), true, Verdict::Refuse),
            (sample(10001, 10001, 0o0755), true, Verdict::Refuse),
            (sample(10001, 10001, 0o0777), true, Verdict::Refuse),
        ];
        for (stat, heal, expected) in cases {
            assert_eq!(judge(stat, owner, heal), expected);
        }
    }

    #[test]
    fn widths() {
        assert_eq!(bits(0o42770_u16), 0o2770);
        assert_eq!(bits(0o100700_u32), 0o0700);
    }
}
