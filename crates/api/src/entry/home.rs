use crate::doctor::{Check, Status};
use locus_api::access::Readers;
use locus_api::registry::Registry;
use locus_api::retention::Retention;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

const SQLITE: &[u8; 16] = b"SQLite format 3\0";

pub struct Home<'a> {
    path: &'a Path,
}

impl<'a> Home<'a> {
    pub fn new(path: &'a Path) -> Self {
        Self { path }
    }

    pub fn estate(&self) -> Check {
        let path = self.path.join("estate.sqlite3");
        let mut header = [0u8; 16];
        match File::open(&path).and_then(|mut file| file.read_exact(&mut header)) {
            Ok(()) if &header == SQLITE => Check::new(
                "estate",
                Status::Ok,
                format!("{} is a readable SQLite estate", path.display()),
                None,
            ),
            Ok(()) => Check::new(
                "estate",
                Status::Fail,
                format!("{} is not a SQLite database", path.display()),
                Some(
                    "Restore estate.sqlite3 together with column/ from a compatible backup; locus-api will not rebuild a damaged estate.",
                ),
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Check::new(
                "estate",
                Status::Fail,
                format!("{} does not exist", path.display()),
                Some(
                    "Run locus-api serve with this same --home once; serving bootstraps the estate.",
                ),
            ),
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Check::new(
                "estate",
                Status::Fail,
                format!("{} is truncated", path.display()),
                Some(
                    "Restore estate.sqlite3 together with column/ from a compatible backup; locus-api will not rebuild a damaged estate.",
                ),
            ),
            Err(error) => Check::new(
                "estate",
                Status::Fail,
                format!("cannot read {}: {error}", path.display()),
                Some("Restore read access to estate.sqlite3 for the user that runs locus-api."),
            ),
        }
    }

    pub fn column(&self) -> Check {
        let path = self.path.join("column");
        match fs::metadata(&path) {
            Ok(metadata) if !metadata.is_dir() => Check::new(
                "column",
                Status::Fail,
                format!("{} is not a directory", path.display()),
                Some(
                    "Move the stray file aside and restore column/ from the backup that matches estate.sqlite3.",
                ),
            ),
            Ok(metadata) if metadata.permissions().readonly() => Check::new(
                "column",
                Status::Fail,
                format!("{} is not writable", path.display()),
                Some(
                    "Restore write access to column/ for the user that runs locus-api; sealing parts writes there.",
                ),
            ),
            Ok(_) => Check::new(
                "column",
                Status::Ok,
                format!("{} is a writable directory", path.display()),
                None,
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Check::new(
                "column",
                Status::Fail,
                format!("{} does not exist", path.display()),
                Some(
                    "Run locus-api serve with this same --home; serving creates column/. If estate.sqlite3 already holds parts, restore column/ from the matching backup instead.",
                ),
            ),
            Err(error) => Check::new(
                "column",
                Status::Fail,
                format!("cannot inspect {}: {error}", path.display()),
                Some("Restore access to column/ for the user that runs locus-api."),
            ),
        }
    }

    pub fn spools(&self) -> Check {
        let path = self.path.join("spools.json");
        if !path.exists() {
            return Check::new(
                "spools",
                Status::Warn,
                format!(
                    "{} does not exist; no product has registered a buffer",
                    path.display()
                ),
                Some(
                    "Run a product that declares the api reporter against this endpoint; registration writes spools.json.",
                ),
            );
        }
        match Registry::open(&path) {
            Ok(registry) => {
                let sources = registry.sources();
                let missing: Vec<String> = sources
                    .iter()
                    .filter(|source| !source.path.is_dir())
                    .map(|source| format!("{} ({})", source.path.display(), source.producer.text()))
                    .collect();
                if missing.is_empty() {
                    Check::new(
                        "spools",
                        Status::Ok,
                        format!("{} registers {} buffer(s)", path.display(), sources.len()),
                        None,
                    )
                } else {
                    Check::new(
                        "spools",
                        Status::Warn,
                        format!(
                            "{} registers {} buffer(s); missing: {}",
                            path.display(),
                            sources.len(),
                            missing.join(", ")
                        ),
                        Some(
                            "A missing buffer only means that product has not run since its buffer was removed; it re-registers on its next run.",
                        ),
                    )
                }
            }
            Err(error) => Check::new(
                "spools",
                Status::Fail,
                error,
                Some(
                    "Fix or move spools.json aside; products re-register their buffers on their next run.",
                ),
            ),
        }
    }

    pub fn config(&self) -> Check {
        let path = self.path.join("locus-api.toml");
        let readers = match Readers::read(self.path) {
            Ok(readers) => readers,
            Err(_) if Retention::read(self.path).is_err() => Readers::default(),
            Err(error) => {
                return Check::new(
                    "config",
                    Status::Fail,
                    error,
                    Some(
                        "Give each [[reader]] a unique name and the unique 64-hex SHA-256 digest of its token, or remove the readers to leave the read API open; serve refuses to start with it as is.",
                    ),
                );
            }
        };
        let guard = match readers.names().as_slice() {
            [] => "; the read API is open".to_string(),
            names => format!("; readers {} guard the read API", names.join(", ")),
        };
        match Retention::read(self.path) {
            Ok(retention) => match retention.declared() {
                Some(days) => Check::new(
                    "config",
                    Status::Ok,
                    format!("{} keeps {days} day(s) of history{guard}", path.display()),
                    None,
                ),
                None if path.is_file() => Check::new(
                    "config",
                    Status::Ok,
                    format!(
                        "{} declares no retention; history is kept{guard}",
                        path.display()
                    ),
                    None,
                ),
                None => Check::new(
                    "config",
                    Status::Ok,
                    format!("{} is absent; history is kept{guard}", path.display()),
                    None,
                ),
            },
            Err(error) => Check::new(
                "config",
                Status::Fail,
                error,
                Some(
                    "Fix locus-api.toml to hold only [retention] days = <whole days>, or remove it to keep all history; serve refuses to start with it as is.",
                ),
            ),
        }
    }
}
