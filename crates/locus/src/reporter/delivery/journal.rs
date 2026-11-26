use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS ledger (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    bytes INTEGER NOT NULL CHECK(bytes >= 0),
    barrier INTEGER NOT NULL
);
INSERT OR IGNORE INTO ledger VALUES (1,0,0);
CREATE TABLE IF NOT EXISTS records (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT, body BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS flight (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    last INTEGER NOT NULL, token TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS capacity BEFORE INSERT ON records
WHEN (SELECT bytes FROM ledger) + length(NEW.body) > 536870912
BEGIN SELECT RAISE(ABORT, 'HTTPS buffer is full; pending Atoms retained'); END;
CREATE TRIGGER IF NOT EXISTS publication AFTER INSERT ON records
BEGIN UPDATE ledger SET bytes = bytes + length(NEW.body); END;
CREATE TRIGGER IF NOT EXISTS retirement AFTER DELETE ON records
BEGIN UPDATE ledger SET bytes = bytes - length(OLD.body); END;";

pub(in crate::reporter) struct Journal {
    dir: PathBuf,
    db: Mutex<Option<Connection>>,
}

impl Journal {
    pub(in crate::reporter) fn open(dir: &Path) -> Result<Self, String> {
        let db = Self::connect(dir)?;
        Ok(Self {
            dir: dir.into(),
            db: Mutex::new(Some(db)),
        })
    }

    fn connect(dir: &Path) -> Result<Connection, String> {
        let db = open(dir, true)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(message)?;
        let version = db
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .map_err(message)?;
        if version > 1 {
            return Err("unsupported HTTPS journal format".into());
        }
        if version == 0 {
            db.execute_batch(&format!(
                "BEGIN IMMEDIATE; {SCHEMA} PRAGMA user_version=1; COMMIT;"
            ))
            .map_err(message)?;
        }
        Ok(db)
    }

    pub(in crate::reporter) fn append(&self, body: &[u8]) -> Result<(), String> {
        let mut held = self.db.lock().map_err(|_| "HTTPS journal lock failed")?;
        if held.is_none() {
            *held = Some(Self::connect(&self.dir)?);
        }
        held.as_ref()
            .ok_or("HTTPS journal closed")?
            .execute("INSERT INTO records(body) VALUES (?1)", [body])
            .map_err(message)?;
        Ok(())
    }

    pub(in crate::reporter) fn extend(&self, records: &[Vec<u8>]) -> Result<(), String> {
        if records.is_empty() {
            return Ok(());
        }
        let mut held = self.db.lock().map_err(|_| "HTTPS journal lock failed")?;
        if held.is_none() {
            *held = Some(Self::connect(&self.dir)?);
        }
        let db = held.as_mut().ok_or("HTTPS journal closed")?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(message)?;
        for body in records {
            tx.execute("INSERT INTO records(body) VALUES (?1)", [body.as_slice()])
                .map_err(message)?;
        }
        tx.commit().map_err(message)
    }

    pub(in crate::reporter) fn pending(&self) -> Result<bool, String> {
        let mut held = self.db.lock().map_err(|_| "HTTPS journal lock failed")?;
        if held.is_none() {
            *held = Some(Self::connect(&self.dir)?);
        }
        held.as_ref()
            .ok_or("HTTPS journal closed")?
            .query_row("SELECT EXISTS(SELECT 1 FROM records)", [], |row| {
                row.get::<_, bool>(0)
            })
            .map_err(message)
    }

    pub(in crate::reporter) fn seal(&self) -> Result<(), String> {
        let db = self
            .db
            .lock()
            .map_err(|_| "HTTPS journal lock failed")?
            .take();
        if let Some(db) = db {
            db.execute_batch("PRAGMA synchronous=FULL; UPDATE ledger SET barrier=1-barrier;")
                .map_err(message)?;
            std::thread::Builder::new()
                .name("locus-journal".into())
                .spawn(move || drop(db))
                .map_err(|_| "cannot close HTTPS journal")?;
        }
        Ok(())
    }
}

pub(super) fn open(dir: &Path, create: bool) -> Result<Connection, String> {
    let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if create {
        flags |= OpenFlags::SQLITE_OPEN_CREATE;
    }
    let db = Connection::open_with_flags(dir.join("queue.db"), flags).map_err(message)?;
    db.busy_timeout(Duration::from_secs(5)).map_err(message)?;
    Ok(db)
}

fn message(error: rusqlite::Error) -> String {
    format!("cannot commit HTTPS journal: {error}")
}
