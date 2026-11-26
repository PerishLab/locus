use super::{ACTIVE, AGE, CLAIMED, Report, Spool, listed, open, segments};
use crate::Error;
use serde_json::Value;
use std::fs::{self, File};
use std::io;
use std::path::Path;

const MARKER: &str = "local";
const MIB: u64 = 1024 * 1024;

pub(in crate::reporter) fn build(options: &Value) -> Result<Box<dyn Report>, Error> {
    let object = options
        .as_object()
        .filter(|object| object.len() == 2)
        .ok_or_else(|| Error::config("local reporter options do not match its schema"))?;
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| Error::config("local reporter requires path"))?;
    let mib = object
        .get("mib")
        .and_then(Value::as_u64)
        .filter(|mib| (2..=512).contains(mib))
        .ok_or_else(|| Error::config("local reporter requires mib between 2 and 512"))?;
    let mut spool = open(path, mib * MIB, MIB)?.aged(AGE);
    spool.local = true;
    spool
        .with(|lock| spool.adopt(lock).map(|()| None))
        .map_err(Error::reporter)?;
    Ok(Box::new(spool))
}

pub fn marked(directory: &Path) -> io::Result<bool> {
    directory.join(MARKER).try_exists()
}

impl Spool {
    fn adopt(&self, lock: &File) -> Result<(), String> {
        let directory = self.directory();
        idle(directory).map_err(|error| error.to_string())?;
        self.root.open(MARKER, false)?;
        self.root.bound(0)?;
        let held = active(directory).map_err(|error| error.to_string())?;
        if held >= self.segment {
            self.root.seal()?;
            self.trim(lock, 0)?;
        } else {
            self.trim(lock, held)?;
        }
        Ok(())
    }

    pub(super) fn retain(&self, held: u64, mut sealed: u64) -> Result<u64, String> {
        for path in segments(self.directory()).map_err(|error| error.to_string())? {
            if held + sealed <= self.ceiling {
                break;
            }
            let bytes = fs::metadata(&path)
                .map_err(|error| error.to_string())?
                .len();
            fs::remove_file(path).map_err(|error| error.to_string())?;
            sealed = sealed.saturating_sub(bytes);
        }
        Ok(sealed)
    }
}

fn active(directory: &Path) -> io::Result<u64> {
    match fs::metadata(directory.join(ACTIVE)) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn idle(directory: &Path) -> io::Result<()> {
    if directory.join("delivery.json").try_exists()?
        || directory.join("registration").try_exists()?
        || !listed(directory, CLAIMED)?.is_empty()
    {
        return Err(io::Error::other("local reporter requires its own buffer"));
    }
    Ok(())
}
