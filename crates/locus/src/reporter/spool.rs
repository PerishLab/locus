use super::Report;
use crate::{Atom, Error};
use serde_json::json;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ACTIVE: &str = "active.jsonl";
const LEDGER: &str = "loss.jsonl";
const LOCK: &str = "spool.lock";
const SEALED: &str = "sealed-";
const CLAIMED: &str = "claimed-";
const SUFFIX: &str = ".jsonl";

pub fn segments(directory: &Path) -> io::Result<Vec<PathBuf>> {
    listed(directory, SEALED)
}

pub fn claim(directory: &Path) -> io::Result<Vec<PathBuf>> {
    for path in listed(directory, SEALED)? {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let target = directory.join(name.replacen(SEALED, CLAIMED, 1));
        match fs::rename(&path, &target) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    listed(directory, CLAIMED)
}

fn listed(directory: &Path, prefix: &str) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(paths),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let stamp = name
            .to_str()
            .and_then(|name| name.strip_prefix(prefix))
            .and_then(|name| name.strip_suffix(SUFFIX));
        if stamp.is_some_and(stamped) {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn stamped(stamp: &str) -> bool {
    !stamp.is_empty() && stamp.bytes().all(|byte| byte.is_ascii_digit())
}

pub(super) fn open(directory: &str, ceiling: u64, segment: u64) -> Result<Spool, Error> {
    let root = Root(PathBuf::from(directory));
    root.create()
        .map_err(|error| Error::reporter(format!("cannot create spool {directory}: {error}")))?;
    let lock = root.open(LOCK, false).map_err(Error::reporter)?;
    Ok(Spool {
        root,
        ceiling,
        segment,
        age: None,
        lock: Mutex::new(lock),
    })
}

pub(super) struct Spool {
    root: Root,
    ceiling: u64,
    segment: u64,
    age: Option<Duration>,
    lock: Mutex<File>,
}

impl Report for Spool {
    fn report(&self, atom: &Atom) -> Result<Option<String>, String> {
        let mut record =
            serde_json::to_vec(atom).map_err(|error| format!("cannot encode Atom: {error}"))?;
        record.push(b'\n');
        let lock = self
            .lock
            .lock()
            .map_err(|_| "spool lock is poisoned".to_string())?;
        lock.lock()
            .map_err(|error| format!("cannot lock spool: {error}"))?;
        let appended = self.append(&lock, &record);
        let unlocked = lock
            .unlock()
            .map_err(|error| format!("cannot unlock spool: {error}"));
        appended.and_then(|loss| unlocked.map(|_| loss))
    }
}

impl Spool {
    pub(super) fn aged(mut self, age: Duration) -> Self {
        self.age = Some(age);
        self
    }

    pub(super) fn directory(&self) -> &Path {
        &self.root.0
    }

    fn append(&self, lock: &File, record: &[u8]) -> Result<Option<String>, String> {
        let mut active = self.root.open(ACTIVE, true)?;
        active
            .write_all(record)
            .and_then(|_| active.flush())
            .map_err(|error| format!("cannot append Atom: {error}"))?;
        let metadata = active.metadata().map_err(|error| error.to_string())?;
        let size = metadata.len();
        let stale = self.age.is_some_and(|age| {
            metadata
                .created()
                .ok()
                .and_then(|created| created.elapsed().ok())
                .is_some_and(|elapsed| elapsed >= age)
        });
        drop(active);
        let known = counted(lock)?;
        let mut sealed = match known {
            Some(sealed) => sealed,
            None => self.root.total()?,
        };
        let mut held = size;
        if size >= self.segment || stale {
            fs::rename(self.root.0.join(ACTIVE), self.root.stamped())
                .map_err(|error| format!("cannot seal spool segment: {error}"))?;
            sealed += size;
            held = 0;
        }
        let mut loss = None;
        if held + sealed > self.ceiling {
            sealed = self.root.total()?;
            (loss, sealed) = self.shed(held, sealed)?;
        }
        if known != Some(sealed) {
            count(lock, sealed)?;
        }
        Ok(loss)
    }

    fn shed(&self, held: u64, mut sealed: u64) -> Result<(Option<String>, u64), String> {
        let (mut records, mut dropped) = (0_u64, 0_u64);
        for path in segments(&self.root.0).map_err(|error| error.to_string())? {
            if held + sealed <= self.ceiling {
                break;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let count = bytes.iter().filter(|byte| **byte == b'\n').count() as u64;
            match fs::remove_file(&path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                removed => removed.map_err(|error| error.to_string())?,
            }
            sealed = sealed.saturating_sub(bytes.len() as u64);
            self.root.ledger(&path, count, bytes.len() as u64)?;
            records += count;
            dropped += 1;
        }
        let loss = (records > 0).then(|| {
            format!(
                "spool over ceiling dropped {records} record(s) in {dropped} sealed segment(s); see {LEDGER}"
            )
        });
        Ok((loss, sealed))
    }
}

fn counted(mut lock: &File) -> Result<Option<u64>, String> {
    let mut text = String::new();
    lock.seek(SeekFrom::Start(0))
        .and_then(|_| lock.read_to_string(&mut text))
        .map_err(|error| format!("cannot read spool counter: {error}"))?;
    Ok(text.trim().parse().ok())
}

fn count(mut lock: &File, sealed: u64) -> Result<(), String> {
    lock.set_len(0)
        .and_then(|_| lock.seek(SeekFrom::Start(0)))
        .and_then(|_| lock.write_all(sealed.to_string().as_bytes()))
        .map_err(|error| format!("cannot write spool counter: {error}"))
}

struct Root(PathBuf);

impl Root {
    fn create(&self) -> io::Result<()> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&self.0)
    }

    fn open(&self, name: &str, append: bool) -> Result<File, String> {
        let mut options = OpenOptions::new();
        options
            .create(true)
            .read(!append)
            .write(!append)
            .append(append);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options
            .open(self.0.join(name))
            .map_err(|error| format!("cannot open spool {}: {error}", self.0.display()))
    }

    fn total(&self) -> Result<u64, String> {
        let mut bytes = 0;
        let sealed = segments(&self.0).map_err(|error| error.to_string())?;
        let claimed = listed(&self.0, CLAIMED).map_err(|error| error.to_string())?;
        for path in sealed.iter().chain(&claimed) {
            bytes += fs::metadata(path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
        }
        Ok(bytes)
    }

    fn stamped(&self) -> PathBuf {
        let mut stamp = now();
        loop {
            let path = self.0.join(format!("{SEALED}{stamp:020}{SUFFIX}"));
            if !path.exists() {
                return path;
            }
            stamp += 1;
        }
    }

    fn ledger(&self, path: &Path, records: u64, bytes: u64) -> Result<(), String> {
        let segment = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let entry = json!({"at": now(), "segment": segment, "records": records, "bytes": bytes});
        let mut file = self.open(LEDGER, true)?;
        file.write_all(format!("{entry}\n").as_bytes())
            .and_then(|_| file.flush())
            .map_err(|error| format!("cannot record loss: {error}"))
    }
}

fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
