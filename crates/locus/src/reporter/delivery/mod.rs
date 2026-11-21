mod batch;
pub(super) mod journal;
pub(super) mod storage;

pub mod transport;
use batch::Batch;
use radicula_lock::{Factory, Policy, Wait};
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    version: u8,
    format: String,
    producer: String,
    endpoint: String,
    #[serde(rename = "key_file")]
    credential: PathBuf,
}

impl Binding {
    fn read(directory: &Path) -> Result<Self, String> {
        let bytes = fs::read(directory.join("delivery.json"))
            .map_err(|_| "cannot read HTTPS delivery binding")?;
        let binding: Self =
            serde_json::from_slice(&bytes).map_err(|_| "invalid HTTPS delivery binding")?;
        let supported = binding.version == 1 && binding.format == "sqlite-wal/v1";
        if !supported || binding.producer.is_empty() || !binding.credential.is_absolute() {
            return Err("invalid HTTPS delivery binding".into());
        }
        Ok(binding)
    }

    fn address(&self) -> Result<Url, String> {
        let mut url = transport::origin(&self.endpoint)?;
        if url.scheme() != "https" {
            return Err("delivery requires HTTPS".into());
        }
        url.set_path("/api/v1/atoms");
        url.query_pairs_mut()
            .append_pair("producer", &self.producer);
        Ok(url)
    }
}

pub fn send(directory: &Path) -> Result<usize, String> {
    if !directory.is_absolute() {
        return Err("delivery buffer must be absolute".into());
    }
    if !directory.try_exists().map_err(|error| error.to_string())? {
        return Ok(0);
    }
    #[cfg(unix)]
    if !private(directory).map_err(|_| "cannot inspect HTTPS buffer")? {
        return Err("HTTPS buffer directory must be private".into());
    }
    let binding = Binding::read(directory)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .read(true)
        .open(directory.join("sender.lock"))
        .map_err(|error| error.to_string())?;
    let gate = Factory::file(file, Policy::EXCLUSIVE).map_err(|error| error.to_string())?;
    match gate.with(Wait::timeout(std::time::Duration::from_secs(5)), || {
        drain(directory, &binding, &transport::agent())
    }) {
        Ok(result) => result,
        Err(error) => Err(format!("sender lock refused: {error}")),
    }
}

#[cfg(unix)]
pub(super) fn private(directory: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::symlink_metadata(directory)?;
    Ok(meta.is_dir() && meta.permissions().mode() & 0o077 == 0)
}

fn drain(directory: &Path, binding: &Binding, agent: &ureq::Agent) -> Result<usize, String> {
    let url = binding.address()?;
    let mut db = journal::open(directory, false)?;
    db.execute_batch("PRAGMA synchronous=FULL")
        .map_err(|e| e.to_string())?;
    let mut count = 0;
    for _ in 0..16 {
        let Some(batch) = Batch::open(&mut db)? else {
            break;
        };
        let key = transport::secret(&binding.credential)?;
        let response = agent
            .post(url.as_str())
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(15)))
            .build()
            .header("authorization", format!("Bearer {key}"))
            .header("idempotency-key", &batch.token)
            .content_type("application/x-ndjson")
            .send(batch.bytes.as_slice())
            .map_err(|_| "HTTPS request failed; batch retained")?;
        if response.status().as_u16() != 204 {
            return Err(format!(
                "HTTPS delivery returned {}; batch retained",
                response.status().as_u16()
            ));
        }
        count += batch.confirm(&mut db).map_err(|error| error.to_string())?;
    }
    Ok(count)
}
