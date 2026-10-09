use crate::registry::Registry;
use crate::store::{Producer, Record, Store};
use locus::reporter::spool;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const OFFSET: &str = ".offset";
const BATCH: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Source {
    pub producer: Producer,
    pub path: PathBuf,
}

pub struct Drain {
    store: Arc<dyn Store>,
    rejected: PathBuf,
    registry: Arc<Registry>,
}

impl Drain {
    pub fn new(store: Arc<dyn Store>, rejected: PathBuf, registry: Arc<Registry>) -> Self {
        Self {
            store,
            rejected,
            registry,
        }
    }

    pub fn cycle(&self) -> Result<(), String> {
        for source in &self.registry.sources() {
            let claimed = spool::claim(&source.path)
                .map_err(|error| format!("cannot claim {}: {error}", source.path.display()))?;
            for path in claimed {
                self.advance(source, &path)?;
                fs::remove_file(&path).map_err(|error| error.to_string())?;
                remove(&sibling(&path, OFFSET))?;
            }
        }
        Ok(())
    }

    fn advance(&self, source: &Source, path: &Path) -> Result<u64, String> {
        let marker = sibling(path, OFFSET);
        let mut offset = position(&marker)?;
        loop {
            let consumed = self.batch(source, path, offset)?;
            if consumed == 0 {
                return Ok(offset);
            }
            offset += consumed;
            mark(&marker, offset)?;
        }
    }

    fn batch(&self, source: &Source, path: &Path, offset: u64) -> Result<u64, String> {
        let mut file =
            File::open(path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| error.to_string())?;
        let mut buffer = Vec::new();
        file.take(BATCH)
            .read_to_end(&mut buffer)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let complete = buffer
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        if complete == 0 && buffer.len() as u64 == BATCH {
            return Err(format!(
                "{} holds a record over {BATCH} bytes",
                path.display()
            ));
        }
        let mut records = Vec::new();
        let mut rejected = Vec::new();
        for line in buffer[..complete].split(|byte| *byte == b'\n') {
            if line.is_empty() {
                continue;
            }
            match Record::parse(line) {
                Ok(record) => records.push(record),
                Err(_) => rejected.push(line),
            }
        }
        self.store.append(&source.producer, &records)?;
        self.reject(&source.producer, &rejected)?;
        Ok(complete as u64)
    }

    fn reject(&self, producer: &Producer, lines: &[&[u8]]) -> Result<(), String> {
        if lines.is_empty() {
            return Ok(());
        }
        eprintln!(
            "locus-api: rejected {} malformed record(s) from {producer}",
            lines.len()
        );
        fs::create_dir_all(&self.rejected).map_err(|error| error.to_string())?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.rejected.join(format!("{producer}.jsonl")))
            .map_err(|error| error.to_string())?;
        for line in lines {
            file.write_all(&[line, &b"\n"[..]].concat())
                .map_err(|error| error.to_string())?;
        }
        file.sync_data().map_err(|error| error.to_string())
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn position(marker: &Path) -> Result<u64, String> {
    match fs::read_to_string(marker) {
        Ok(text) => text
            .trim()
            .parse()
            .map_err(|error| format!("invalid offset in {}: {error}", marker.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(format!("cannot read {}: {error}", marker.display())),
    }
}

fn mark(marker: &Path, offset: u64) -> Result<(), String> {
    let staging = sibling(marker, ".staging");
    let mut file = File::create(&staging).map_err(|error| error.to_string())?;
    file.write_all(offset.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())?;
    fs::rename(&staging, marker).map_err(|error| error.to_string())
}

fn remove(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}
