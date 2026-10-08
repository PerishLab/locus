use crate::day;
use locus::{Atom, Key, Role};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Producer(String);

impl Producer {
    pub fn new(text: &str) -> Result<Self, String> {
        if sized(text) && !text.starts_with(['.', '-']) && text.bytes().all(allowed) {
            Ok(Self(text.to_string()))
        } else {
            Err(format!("invalid producer {text:?}"))
        }
    }

    pub fn text(&self) -> &str {
        &self.0
    }
}

fn sized(text: &str) -> bool {
    !text.is_empty() && text.len() <= 64
}

fn allowed(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-".contains(&byte)
}

impl fmt::Display for Producer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub struct Record {
    bytes: Vec<u8>,
    at: u64,
}

impl Record {
    pub fn parse(line: &[u8]) -> Result<Self, String> {
        let atom: Atom = serde_json::from_slice(line).map_err(|error| error.to_string())?;
        Ok(Self {
            bytes: line.to_vec(),
            at: atom.at(),
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, Debug, Default)]
pub struct Window {
    pub from: Option<u64>,
    pub to: Option<u64>,
}

impl Window {
    pub fn contains(&self, at: u64) -> bool {
        self.from.is_none_or(|from| at >= from) && self.to.is_none_or(|to| at < to)
    }

    fn covers(&self, day: u64) -> bool {
        self.from.is_none_or(|from| day >= from / day::NANOS)
            && self.to.is_none_or(|to| day.saturating_mul(day::NANOS) < to)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub window: Window,
    pub selector: Option<(Role, Key)>,
    pub producer: Option<Producer>,
}

impl Filter {
    fn admits(&self, atom: &Atom) -> bool {
        self.window.contains(atom.at())
            && self.selector.as_ref().is_none_or(|(role, key)| {
                atom.context().get(role.text()).map(String::as_str) == Some(key.text())
            })
    }
}

pub type Sink<'a> = dyn FnMut(&[u8]) -> Result<(), String> + 'a;

pub trait Store: Send + Sync {
    fn append(&self, producer: &Producer, records: &[Record]) -> Result<(), String>;
    fn read(&self, filter: &Filter, sink: &mut Sink<'_>) -> Result<(), String>;
    fn expire(&self, producer: &Producer, day: u64) -> Result<(), String>;
}

pub struct Segments {
    root: PathBuf,
    lock: Mutex<()>,
}

impl Segments {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, String> {
        let root = root.into();
        fs::create_dir_all(&root)
            .map_err(|error| format!("cannot create store {}: {error}", root.display()))?;
        Ok(Self {
            root,
            lock: Mutex::new(()),
        })
    }

    fn path(&self, producer: &Producer, day: u64) -> PathBuf {
        self.root
            .join(producer.text())
            .join(format!("{}.jsonl", day::civil(day)))
    }

    fn partitions(&self, filter: &Filter) -> Result<Vec<(u64, Producer, PathBuf)>, String> {
        let mut partitions = Vec::new();
        for producer in listing(&self.root)? {
            let Ok(name) = Producer::new(&producer) else {
                continue;
            };
            if filter
                .producer
                .as_ref()
                .is_some_and(|wanted| wanted != &name)
            {
                continue;
            }
            for file in listing(&self.root.join(name.text()))? {
                let day = file.strip_suffix(".jsonl").and_then(day::parse);
                if let Some(day) = day.filter(|day| filter.window.covers(*day)) {
                    partitions.push((day, name.clone(), self.path(&name, day)));
                }
            }
        }
        partitions.sort();
        Ok(partitions)
    }
}

impl Store for Segments {
    fn append(&self, producer: &Producer, records: &[Record]) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "store lock is poisoned".to_string())?;
        let mut open: Option<(u64, File)> = None;
        for record in records {
            let day = record.at / day::NANOS;
            if open.as_ref().is_none_or(|(current, _)| *current != day) {
                if let Some((_, file)) = open.take() {
                    file.sync_data().map_err(|error| error.to_string())?;
                }
                open = Some((day, segment(&self.path(producer, day))?));
            }
            let (_, file) = open.as_mut().expect("open segment");
            file.write_all(&[record.bytes(), b"\n"].concat())
                .map_err(|error| format!("cannot append record: {error}"))?;
        }
        if let Some((_, file)) = open {
            file.sync_data().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn read(&self, filter: &Filter, sink: &mut Sink<'_>) -> Result<(), String> {
        for (_, _, path) in self.partitions(filter)? {
            let file = File::open(&path)
                .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
            let mut reader = BufReader::new(file);
            let mut line = Vec::new();
            loop {
                line.clear();
                let bytes = reader
                    .read_until(b'\n', &mut line)
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
                if bytes == 0 || line.last() != Some(&b'\n') {
                    break;
                }
                let atom: Atom =
                    serde_json::from_slice(&line[..line.len() - 1]).map_err(|error| {
                        format!("stored record in {} is invalid: {error}", path.display())
                    })?;
                if filter.admits(&atom) {
                    sink(&line)?;
                }
            }
        }
        Ok(())
    }

    fn expire(&self, producer: &Producer, day: u64) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "store lock is poisoned".to_string())?;
        match fs::remove_file(self.path(producer, day)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.to_string()),
            _ => Ok(()),
        }
    }
}

fn segment(path: &Path) -> Result<File, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("cannot open {}: {error}", path.display()))
}

fn listing(path: &Path) -> Result<Vec<String>, String> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot list {}: {error}", path.display())),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}
