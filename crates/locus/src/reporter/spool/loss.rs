use super::{LEDGER, Root, Spool, now, segments};
use serde_json::json;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

const LIMIT: u64 = 1024 * 1024;

impl Spool {
    pub(super) fn shed(&self, held: u64, mut sealed: u64) -> Result<(Option<String>, u64), String> {
        let (mut records, mut dropped) = (0_u64, 0_u64);
        for path in segments(&self.root.0).map_err(|error| error.to_string())? {
            if held + sealed <= self.ceiling {
                break;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let count = bytes.iter().filter(|byte| **byte == b'\n').count() as u64;
            match fs::remove_file(&path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                removed => removed.map_err(|error| error.to_string())?,
            }
            sealed = sealed.saturating_sub(bytes.len() as u64);
            self.root.ledger(&path, count, bytes.len() as u64)?;
            records += count;
            dropped += 1;
        }
        let loss = (records > 0).then(|| {
            format!(
                "spool over ceiling dropped {records} record(s) in {dropped} sealed segment(s); see {LEDGER}"
            )
        });
        Ok((loss, sealed))
    }
}

impl Root {
    fn ledger(&self, path: &Path, records: u64, bytes: u64) -> Result<(), String> {
        let segment = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let entry = json!({"at": now(), "segment": segment, "records": records, "bytes": bytes});
        let line = format!("{entry}\n");
        self.bound(line.len() as u64)?;
        let mut file = self.open(LEDGER, true)?;
        file.write_all(line.as_bytes())
            .and_then(|_| file.flush())
            .map_err(|error| format!("cannot record loss: {error}"))
    }

    pub(super) fn bound(&self, incoming: u64) -> Result<(), String> {
        match fs::metadata(self.0.join(LEDGER)) {
            Ok(metadata) if metadata.len().saturating_add(incoming) > LIMIT => self
                .open(LEDGER, false)?
                .set_len(0)
                .map_err(|error| format!("cannot rotate loss ledger: {error}")),
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("cannot inspect loss ledger: {error}")),
        }
    }
}
