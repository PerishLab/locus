use crate::store::Producer;
use crate::takeover::Source;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const LOCK: &str = "spool.lock";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Entry {
    producer: String,
    path: PathBuf,
}

pub struct Registry {
    file: PathBuf,
    entries: Mutex<Vec<Entry>>,
}

impl Registry {
    pub fn open(file: impl Into<PathBuf>) -> Result<Self, String> {
        let file = file.into();
        let entries = match fs::read(&file) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| format!("invalid registry {}: {error}", file.display()))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(format!("cannot read {}: {error}", file.display())),
        };
        Ok(Self {
            file,
            entries: Mutex::new(entries),
        })
    }

    pub fn enroll(&self, producer: &str, path: &Path) -> Result<(), String> {
        let producer = Producer::new(producer)?;
        if !path.is_absolute() || !path.join(LOCK).is_file() {
            return Err(format!("{} is not a Locus spool", path.display()));
        }
        let path = fs::canonicalize(path).map_err(|error| error.to_string())?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "registry lock is poisoned".to_string())?;
        if entries
            .iter()
            .any(|entry| entry.path == path && entry.producer == producer.text())
        {
            return Ok(());
        }
        entries.retain(|entry| entry.path != path);
        entries.push(Entry {
            producer: producer.text().to_string(),
            path,
        });
        let staging = self.file.with_extension("staging");
        let bytes = serde_json::to_vec_pretty(&*entries).map_err(|error| error.to_string())?;
        fs::write(&staging, bytes)
            .and_then(|_| fs::rename(&staging, &self.file))
            .map_err(|error| format!("cannot persist {}: {error}", self.file.display()))
    }

    pub fn sources(&self) -> Vec<Source> {
        let entries = match self.entries.lock() {
            Ok(entries) => entries.clone(),
            Err(_) => return Vec::new(),
        };
        entries
            .into_iter()
            .filter_map(|entry| {
                Some(Source {
                    producer: Producer::new(&entry.producer).ok()?,
                    path: entry.path,
                })
            })
            .collect()
    }
}
