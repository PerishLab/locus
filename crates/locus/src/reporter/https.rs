use super::delivery::journal::Journal;
use super::worker::Worker;
use super::{Report, spool};
use crate::delivery::transport;
use crate::{Atom, Error};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

const CEILING: u64 = 512 * 1024 * 1024;
const LINE: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    endpoint: String,
    path: PathBuf,
    #[serde(rename = "key_file")]
    credential: PathBuf,
    #[serde(default, deserialize_with = "sender")]
    sender: Option<PathBuf>,
}

pub(super) fn build(options: &Value, producer: Option<&str>) -> Result<Box<dyn Report>, Error> {
    let options: Options = serde_json::from_value(options.clone())
        .map_err(|_| Error::config("https reporter requires endpoint, key_file and path"))?;
    let endpoint = transport::origin(&options.endpoint).map_err(Error::config)?;
    if endpoint.scheme() != "https" {
        return Err(Error::config("https reporter requires an HTTPS origin"));
    }
    if !options.path.is_absolute() || !options.credential.is_absolute() {
        return Err(Error::config("https reporter paths must be absolute"));
    }
    let producer = producer.ok_or_else(|| Error::config("https reporter requires a producer"))?;
    let directory = options
        .path
        .to_str()
        .ok_or_else(|| Error::config("invalid buffer path"))?;
    let delivery = json!({"version":1,"format":"sqlite-wal/v1","producer":producer,"endpoint":endpoint.origin().ascii_serialization(),"key_file":options.credential});
    let spool = spool::open(directory, CEILING, 0)?;
    spool.bind(&delivery).map_err(Error::reporter)?;
    if options
        .sender
        .as_ref()
        .is_some_and(|p| !super::launcher::trusted(p))
    {
        return Err(Error::config(
            "HTTPS sender must be a trusted absolute executable",
        ));
    }
    let journal = Journal::open(&options.path).map_err(Error::reporter)?;
    Ok(Box::new(Https {
        journal,
        dirty: AtomicBool::new(false),
        worker: Worker::new(options.path, options.sender),
    }))
}

fn sender<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Option<PathBuf>, D::Error> {
    PathBuf::deserialize(de).map(Some)
}
struct Https {
    journal: Journal,
    dirty: AtomicBool,
    worker: Worker,
}

impl Drop for Https {
    fn drop(&mut self) {
        let _ = self.settle();
    }
}

impl Report for Https {
    fn report(&self, atom: &Atom) -> Result<Option<String>, String> {
        let mut record = serde_json::to_vec(atom).map_err(|error| error.to_string())?;
        record.push(b'\n');
        if record.len() > LINE {
            return Err("HTTPS Atom exceeds the 1 MiB single-line limit".into());
        }
        self.journal.append(&record)?;
        self.dirty.store(true, Ordering::Release);
        self.worker.published();
        Ok(None)
    }

    fn settle(&self) -> Result<(), String> {
        if self.dirty.swap(false, Ordering::AcqRel) {
            let barrier = self.journal.seal();
            let sent = self.worker.finish();
            barrier.and(sent)?;
        }
        Ok(())
    }
}
