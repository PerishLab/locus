use super::state::{Phase, Proof, State};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(super) const LIMIT: usize = 2048;

#[derive(Deserialize, Serialize)]
pub(super) struct Ledger {
    version: u8,
    pub interval: u64,
    pub delay: u64,
    pub rules: BTreeMap<String, State>,
    pub notices: Vec<Notice>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Notice {
    pub id: String,
    pub rule: String,
    pub revision: u64,
    pub producer: String,
    pub recipient: String,
    pub phase: Phase,
    pub proof: Proof,
    pub link: String,
    pub delivery: Delivery,
    pub attempts: u32,
    pub retry: u64,
    pub http: Option<u16>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Delivery {
    Pending,
    Sent,
    Refused,
    Muted,
}

pub(super) struct Seat {
    path: PathBuf,
    held: File,
}

impl Seat {
    pub fn open(home: &Path) -> Result<Self, String> {
        let path = home.join("alerts");
        fs::create_dir_all(&path).map_err(text)?;
        let held = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.join("active"))
            .map_err(text)?;
        radicula_lock::attempt(&held, radicula_lock::Mode::Exclusive)
            .map_err(|error| format!("alert evaluator seat unavailable: {error}"))?;
        Ok(Self { path, held })
    }

    pub fn load(&self) -> Result<Ledger, String> {
        let document = read(&self.path)?;
        Ok(document.unwrap_or_else(Ledger::empty))
    }

    pub fn save(&self, ledger: &Ledger) -> Result<(), String> {
        let _ = &self.held;
        let stage = self.path.join("stage");
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&stage).map_err(text)?;
        serde_json::to_writer(&mut file, ledger).map_err(|error| error.to_string())?;
        file.flush().map_err(text)?;
        file.sync_all().map_err(text)?;
        drop(file);
        fs::rename(stage, self.path.join("state.json")).map_err(text)?;
        sync(&self.path).map_err(text)
    }
}

impl Ledger {
    fn empty() -> Self {
        Self {
            version: 1,
            interval: 0,
            delay: 0,
            rules: BTreeMap::new(),
            notices: Vec::new(),
        }
    }

    pub fn room(&mut self, needed: usize) -> Result<(), String> {
        while self.notices.len() + needed > LIMIT {
            let Some(old) = self
                .notices
                .iter()
                .position(|notice| notice.delivery != Delivery::Pending)
            else {
                return Err("alert outbox is full".into());
            };
            self.notices.remove(old);
        }
        Ok(())
    }
}

pub(super) fn status(home: &Path, now: u64) -> Result<serde_json::Value, String> {
    let Some(ledger) = read(&home.join("alerts"))? else {
        return Ok(serde_json::json!({"evaluator":"unobserved","backlog":0}));
    };
    let mut stale = 0;
    for state in ledger.rules.values() {
        let elapsed = now
            .saturating_sub(ledger.delay)
            .saturating_sub(state.evaluated);
        if elapsed > ledger.interval.saturating_mul(2) {
            stale += 1;
        }
    }
    let pending = ledger
        .notices
        .iter()
        .filter(|notice| notice.delivery == Delivery::Pending)
        .count();
    let stopped = if stale > 0 { "stopped" } else { "running" };
    Ok(
        serde_json::json!({"evaluator":stopped,"stale_rules":stale,"backlog":pending,"state":ledger}),
    )
}

fn read(directory: &Path) -> Result<Option<Ledger>, String> {
    let path = directory.join("state.json");
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(text(error)),
    };
    if file.metadata().map_err(text)?.len() > 16 * 1024 * 1024 {
        return Err("alert ledger exceeds 16 MiB".into());
    }
    let ledger: Ledger = serde_json::from_reader(file).map_err(|error| error.to_string())?;
    if ledger.version != 1 || ledger.notices.len() > LIMIT {
        return Err("invalid alert ledger".into());
    }
    Ok(Some(ledger))
}

fn text(error: io::Error) -> String {
    error.to_string()
}

#[cfg(unix)]
fn sync(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync(_: &Path) -> io::Result<()> {
    Ok(())
}
