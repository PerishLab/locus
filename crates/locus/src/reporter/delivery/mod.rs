mod batch;
pub(super) mod journal;
pub(in crate::reporter) mod privacy;
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

#[derive(Clone)]
pub(in crate::reporter) struct Route {
    pub(in crate::reporter) endpoint: Url,
    pub(in crate::reporter) producer: String,
    pub(in crate::reporter) key: String,
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
        let url = transport::origin(&self.endpoint)?;
        if url.scheme() != "https" {
            return Err("delivery requires HTTPS".into());
        }
        Ok(destination(&url, &self.producer))
    }
}

pub fn send(directory: &Path) -> Result<usize, String> {
    let buffer = Buffer(directory);
    if !buffer.ready()? {
        return Ok(0);
    }
    let binding = Binding::read(directory)?;
    let url = binding.address()?;
    buffer
        .deliver(&url, || transport::secret(&binding.credential))
        .map(|(count, _)| count)
}

pub(in crate::reporter) fn push(directory: &Path, route: &Route) -> Result<bool, String> {
    let buffer = Buffer(directory);
    if !buffer.ready()? {
        return Ok(false);
    }
    let url = destination(&route.endpoint, &route.producer);
    buffer
        .deliver(&url, || Ok(route.key.clone()))
        .map(|(_, more)| more)
}

fn destination(origin: &Url, producer: &str) -> Url {
    let mut url = origin.clone();
    url.set_path("/api/v1/atoms");
    url.query_pairs_mut().append_pair("producer", producer);
    url
}

struct Buffer<'a>(&'a Path);

impl Buffer<'_> {
    fn ready(&self) -> Result<bool, String> {
        if !self.0.is_absolute() {
            return Err("delivery buffer must be absolute".into());
        }
        if !self.0.try_exists().map_err(|error| error.to_string())? {
            return Ok(false);
        }
        privacy::verify(self.0)?;
        Ok(true)
    }

    fn deliver(
        &self,
        url: &Url,
        secret: impl Fn() -> Result<String, String>,
    ) -> Result<(usize, bool), String> {
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).write(true).read(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let file = options
            .open(self.0.join("sender.lock"))
            .map_err(|error| error.to_string())?;
        let gate = Factory::file(file, Policy::EXCLUSIVE).map_err(|error| error.to_string())?;
        match gate.with(Wait::timeout(std::time::Duration::from_secs(5)), || {
            self.drain(url, secret, &transport::agent())
        }) {
            Ok(result) => result,
            Err(error) => Err(format!("sender lock refused: {error}")),
        }
    }

    fn drain(
        &self,
        url: &Url,
        secret: impl Fn() -> Result<String, String>,
        agent: &ureq::Agent,
    ) -> Result<(usize, bool), String> {
        let mut db = journal::open(self.0, false)?;
        db.execute_batch("PRAGMA synchronous=FULL")
            .map_err(|e| e.to_string())?;
        let mut count = 0;
        for _ in 0..16 {
            let Some(batch) = Batch::open(&mut db)? else {
                return Ok((count, false));
            };
            let key = secret()?;
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
        let more = db
            .query_row("SELECT EXISTS(SELECT 1 FROM records)", [], |row| {
                row.get::<_, bool>(0)
            })
            .map_err(|e| e.to_string())?;
        Ok((count, more))
    }
}
