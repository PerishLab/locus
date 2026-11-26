use super::{LOCK, Spool};
use crate::reporter::delivery::storage;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::Path;

impl Spool {
    pub(super) fn bind(&self, binding: &Value) -> Result<(), String> {
        crate::reporter::delivery::privacy::verify(self.directory())?;
        let bytes = serde_json::to_vec(binding).map_err(|error| error.to_string())?;
        self.with(|_| {
            let path = self.directory().join("delivery.json");
            match fs::read(&path) {
                Ok(held) if held == bytes => Ok(None),
                Ok(_) => Err("HTTPS buffer is bound to different delivery settings".into()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    dedicated(self.directory())?;
                    storage::install(&path, "publishing.tmp", &bytes).map_err(message)?;
                    Ok(None)
                }
                Err(error) => Err(message(error)),
            }
        })
        .map(|_| ())
    }
}

fn dedicated(dir: &Path) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(message)? {
        let name = entry.map_err(message)?.file_name();
        if name != LOCK && name != "publishing.tmp" {
            return Err("HTTPS reporter requires a new dedicated buffer".into());
        }
    }
    Ok(())
}

fn message(error: io::Error) -> String {
    format!("cannot persist HTTPS buffer: {error}")
}
