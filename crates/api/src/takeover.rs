use crate::store::{Producer, Record, Store};
use locus::reporter::spool;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TAKEN: &str = ".taken.";
const OFFSET: &str = ".offset";
const BATCH: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Source {
    pub producer: Producer,
    pub path: PathBuf,
}

impl Source {
    pub fn new(text: &str) -> Result<Self, String> {
        let (producer, path) = text
            .split_once('=')
            .ok_or_else(|| format!("source {text:?} is not PRODUCER=PATH"))?;
        if path.is_empty() {
            return Err(format!("source {text:?} names no path"));
        }
        Ok(Self {
            producer: Producer::new(producer)?,
            path: PathBuf::from(path),
        })
    }

    fn take(&self) -> Result<(), String> {
        let path = &self.path;
        match fs::metadata(path) {
            Ok(metadata) if metadata.len() > 0 => {
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|error| error.to_string())?
                    .as_nanos();
                let target = sibling(path, &format!("{TAKEN}{stamp:020}"));
                fs::rename(path, &target)
                    .map_err(|error| format!("cannot take {}: {error}", path.display()))
            }
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                Err(format!("cannot inspect {}: {error}", path.display()))
            }
            _ => Ok(()),
        }
    }

    fn taken(&self) -> Result<Vec<PathBuf>, String> {
        let path = &self.path;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
        let parent = parent.unwrap_or(Path::new("."));
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Err(format!("source {} has no file name", path.display()));
        };
        let prefix = format!("{name}{TAKEN}");
        let entries = match fs::read_dir(parent) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("cannot list {}: {error}", parent.display())),
        };
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let file = entry.file_name();
            let Some(file) = file.to_str() else { continue };
            let stamped = file.strip_prefix(&prefix).is_some_and(|stamp| {
                !stamp.is_empty() && stamp.bytes().all(|byte| byte.is_ascii_digit())
            });
            if stamped {
                paths.push(entry.path());
            }
        }
        paths.sort();
        Ok(paths)
    }
}

pub struct Takeover {
    store: Arc<dyn Store>,
    rejected: PathBuf,
    files: Vec<Source>,
    grace: Duration,
    spools: Vec<Source>,
}

impl Takeover {
    pub fn new(store: Arc<dyn Store>, rejected: PathBuf) -> Self {
        Self {
            store,
            rejected,
            files: Vec::new(),
            grace: Duration::ZERO,
            spools: Vec::new(),
        }
    }

    pub fn files(mut self, files: Vec<Source>, grace: Duration) -> Self {
        self.files = files;
        self.grace = grace;
        self
    }

    pub fn spools(mut self, spools: Vec<Source>) -> Self {
        self.spools = spools;
        self
    }

    pub fn cycle(&self) -> Result<(), String> {
        for source in &self.files {
            source.take()?;
            for path in source.taken()? {
                self.drain(source, &path)?;
            }
        }
        for source in &self.spools {
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

    fn drain(&self, source: &Source, path: &Path) -> Result<(), String> {
        let offset = self.advance(source, path)?;
        let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
        let idle = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|elapsed| elapsed >= self.grace);
        if idle && offset == metadata.len() {
            fs::remove_file(path).map_err(|error| error.to_string())?;
            remove(&sibling(path, OFFSET))?;
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
