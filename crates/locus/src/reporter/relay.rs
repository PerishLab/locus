use super::delivery::Route;
use super::delivery::journal::Journal;
use super::delivery::transport;
use super::spool;
use super::worker::Worker;
use crate::Error;
use serde_json::json;
use std::fmt;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use url::Url;

const CEILING: u64 = 512 * 1024 * 1024;
const KEY: usize = 8192;
const LINE: usize = 1024 * 1024;

#[derive(Clone)]
pub struct Config {
    pub endpoint: String,
    pub key: String,
    pub producer: String,
    pub buffer: PathBuf,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("endpoint", &self.endpoint)
            .field("key", &"[redacted]")
            .field("producer", &self.producer)
            .field("buffer", &self.buffer)
            .finish()
    }
}

impl Config {
    pub fn new(
        endpoint: impl Into<String>,
        key: impl Into<String>,
        producer: impl Into<String>,
        buffer: impl Into<PathBuf>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            key: key.into(),
            producer: producer.into(),
            buffer: buffer.into(),
        }
    }
}

pub struct Relay {
    journal: Journal,
    dirty: AtomicBool,
    worker: Worker,
}

impl Relay {
    pub fn open(config: Config) -> Result<Self, Error> {
        let endpoint = transport::origin(&config.endpoint).map_err(Error::config)?;
        if !allowed(&endpoint) {
            return Err(Error::config(
                "relay requires an HTTPS origin or HTTP loopback IP",
            ));
        }
        if !valid(&config.key) {
            return Err(Error::config("relay key must be a nonempty token"));
        }
        if !named(&config.producer) {
            return Err(Error::config("relay requires a valid producer"));
        }
        if !config.buffer.is_absolute() {
            return Err(Error::config("relay buffer path must be absolute"));
        }
        let directory = config
            .buffer
            .to_str()
            .ok_or_else(|| Error::config("invalid buffer path"))?;
        let binding = json!({
            "version": 1,
            "format": "sqlite-wal/v1",
            "mode": "relay",
            "producer": config.producer,
            "endpoint": endpoint.origin().ascii_serialization()
        });
        let _spool = spool::bound(directory, CEILING, &binding)?;
        let journal = Journal::open(&config.buffer).map_err(Error::reporter)?;
        let route = Route {
            endpoint,
            producer: config.producer,
            key: config.key,
        };
        let worker = Worker::direct(config.buffer, route);
        let pending = journal.pending().map_err(Error::reporter)?;
        let dirty = AtomicBool::new(pending);
        if pending {
            worker.published();
        }
        Ok(Self {
            journal,
            dirty,
            worker,
        })
    }

    pub fn enqueue<S: AsRef<str>>(&self, lines: &[S]) -> Result<(), String> {
        if lines.is_empty() {
            return Ok(());
        }
        let mut records = Vec::with_capacity(lines.len());
        for line in lines {
            records.push(encode(line.as_ref())?);
        }
        self.journal.extend(&records)?;
        self.dirty.store(true, Ordering::Release);
        self.worker.published();
        Ok(())
    }

    pub fn push(&self, line: &str) -> Result<(), String> {
        self.enqueue(&[line])
    }

    pub fn settle(&self) -> Result<(), String> {
        if self.dirty.swap(false, Ordering::AcqRel) {
            let barrier = self.journal.seal();
            let sent = self.worker.finish();
            barrier.and(sent)?;
        }
        Ok(())
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.settle();
    }
}

fn encode(line: &str) -> Result<Vec<u8>, String> {
    if line.is_empty() || line.bytes().any(|byte| byte == b'\n' || byte == b'\r') {
        return Err("relay Atom must be a single nonempty line".into());
    }
    if line.len() + 1 > LINE {
        return Err("relay Atom exceeds the 1 MiB single-line limit".into());
    }
    let mut record = Vec::with_capacity(line.len() + 1);
    record.extend_from_slice(line.as_bytes());
    record.push(b'\n');
    Ok(record)
}

fn allowed(url: &Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => url
            .host_str()
            .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback()),
        _ => false,
    }
}

fn valid(key: &str) -> bool {
    !key.is_empty() && key.len() <= KEY && key.bytes().all(|byte| byte.is_ascii_graphic())
}

fn named(producer: &str) -> bool {
    !producer.is_empty()
        && !producer
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}
