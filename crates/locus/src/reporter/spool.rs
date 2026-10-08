use super::Report;
use crate::{Atom, Error};
use serde_json::{Value, json};
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const ACTIVE: &str = "active.jsonl";
const LEDGER: &str = "loss.jsonl";
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

pub(super) fn build(options: &Value) -> Result<Box<dyn Report>, Error> {
    let object = options
        .as_object()
        .filter(|object| object.len() == 3)
        .ok_or_else(|| Error::config("spool reporter options do not match its schema"))?;
    let directory = object
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| Error::config("spool reporter requires path"))?;
    let bound = |name: &str| {
        object
            .get(name)
            .and_then(Value::as_u64)
            .filter(|bytes| *bytes > 0)
            .ok_or_else(|| Error::config(format!("spool reporter requires a positive {name}")))
    };
    let (ceiling, segment) = (bound("ceiling")?, bound("segment")?);
    if segment > ceiling {
        return Err(Error::config("spool segment exceeds its ceiling"));
    }
    let root = Root(PathBuf::from(directory));
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&root.0)
        .map_err(|error| Error::reporter(format!("cannot create spool {directory}: {error}")))?;
    let file = root.open().map_err(Error::reporter)?;
    let sealed = root.total().map_err(Error::reporter)?;
    Ok(Box::new(Spool {
        root,
        ceiling,
        segment,
        state: Mutex::new(State { file, sealed }),
    }))
}

struct State {
    file: File,
    sealed: u64,
}

struct Spool {
    root: Root,
    ceiling: u64,
    segment: u64,
    state: Mutex<State>,
}

impl Report for Spool {
    fn report(&self, atom: &Atom) -> Result<Option<String>, String> {
        let mut record =
            serde_json::to_vec(atom).map_err(|error| format!("cannot encode Atom: {error}"))?;
        record.push(b'\n');
        let mut state = self
            .state
            .lock()
            .map_err(|_| "spool lock is poisoned".to_string())?;
        self.attach(&mut state)?;
        let settled = state
            .file
            .write_all(&record)
            .and_then(|_| state.file.flush())
            .map_err(|error| format!("cannot append Atom: {error}"))
            .and_then(|_| self.settle(&mut state));
        let unlocked = state
            .file
            .unlock()
            .map_err(|error| format!("cannot unlock spool: {error}"));
        settled.and_then(|loss| unlocked.map(|_| loss))
    }
}

impl Spool {
    fn attach(&self, state: &mut State) -> Result<(), String> {
        loop {
            if !self.current(state)? {
                state.file = self.root.open()?;
                state.sealed = self.root.total()?;
                continue;
            }
            state
                .file
                .lock()
                .map_err(|error| format!("cannot lock spool: {error}"))?;
            if self.current(state)? {
                return Ok(());
            }
            state.file.unlock().map_err(|error| error.to_string())?;
        }
    }

    fn current(&self, state: &State) -> Result<bool, String> {
        let held = state.file.metadata().map_err(|error| error.to_string())?;
        let named = fs::metadata(self.root.0.join(ACTIVE)).ok();
        Ok(named.is_some_and(|named| named.ino() == held.ino()))
    }

    fn settle(&self, state: &mut State) -> Result<Option<String>, String> {
        let mut active = state
            .file
            .metadata()
            .map_err(|error| error.to_string())?
            .len();
        if active >= self.segment {
            fs::rename(self.root.0.join(ACTIVE), self.root.stamped())
                .map_err(|error| format!("cannot seal spool segment: {error}"))?;
            state.file = self.root.open()?;
            state.sealed += active;
            active = 0;
        }
        if active + state.sealed <= self.ceiling {
            return Ok(None);
        }
        state.sealed = self.root.total()?;
        self.shed(state, active)
    }

    fn shed(&self, state: &mut State, active: u64) -> Result<Option<String>, String> {
        let (mut records, mut dropped) = (0_u64, 0_u64);
        for path in segments(&self.root.0).map_err(|error| error.to_string())? {
            if active + state.sealed <= self.ceiling {
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
            state.sealed = state.sealed.saturating_sub(bytes.len() as u64);
            self.root.ledger(&path, count, bytes.len() as u64)?;
            records += count;
            dropped += 1;
        }
        Ok((records > 0).then(|| {
            format!(
                "spool over ceiling dropped {records} record(s) in {dropped} sealed segment(s); see {LEDGER}"
            )
        }))
    }
}

struct Root(PathBuf);

impl Root {
    fn open(&self) -> Result<File, String> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(self.0.join(ACTIVE))
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
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(self.0.join(LEDGER))
            .map_err(|error| format!("cannot open loss ledger: {error}"))?;
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
