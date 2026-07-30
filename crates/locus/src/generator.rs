use crate::{Error, Key, Role};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spec {
    name: String,
    options: Value,
}

impl Spec {
    pub fn new(name: impl Into<String>, options: Value) -> Self {
        Self {
            name: name.into(),
            options,
        }
    }

    pub fn random() -> Self {
        Self::new("random", json!({}))
    }

    pub fn shared(path: impl AsRef<Path>) -> Self {
        Self::new(
            "shared-file",
            json!({"path": path.as_ref().to_string_lossy()}),
        )
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn options(&self) -> &Value {
        &self.options
    }
}

pub(crate) trait Generate: Send + Sync {
    fn generate(&self, role: &Role) -> Result<Key, Error>;
}

pub(crate) fn build(spec: &Spec) -> Result<Arc<dyn Generate>, Error> {
    match spec.name() {
        "random" => {
            exact(spec.options(), &[])?;
            Ok(Arc::new(Random))
        }
        "shared-file" => {
            exact(spec.options(), &["path"])?;
            let path = spec
                .options()
                .get("path")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| Error::config("shared-file generator requires path"))?;
            Ok(Arc::new(Shared {
                path: PathBuf::from(path),
            }))
        }
        other => Err(Error::config(format!("unknown generator: {other}"))),
    }
}

fn exact(options: &Value, fields: &[&str]) -> Result<(), Error> {
    let object = options
        .as_object()
        .ok_or_else(|| Error::config("generator options must be an object"))?;
    let mut held: Vec<&str> = object.keys().map(String::as_str).collect();
    held.sort_unstable();
    let mut wanted = fields.to_vec();
    wanted.sort_unstable();
    if held != wanted {
        return Err(Error::config("generator options do not match its schema"));
    }
    Ok(())
}

struct Random;

impl Generate for Random {
    fn generate(&self, role: &Role) -> Result<Key, Error> {
        let _ = role;
        random()
    }
}

fn random() -> Result<Key, Error> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| Error::generator(format!("random generator failed: {error}")))?;
    let mut text = String::with_capacity(32);
    for byte in bytes {
        write!(&mut text, "{byte:02x}")
            .map_err(|error| Error::generator(format!("key rendering failed: {error}")))?;
    }
    Key::new(text).map_err(|error| Error::generator(error.to_string()))
}

struct Shared {
    path: PathBuf,
}

#[derive(Deserialize, Serialize)]
struct Record {
    version: u8,
    key: String,
}

impl Generate for Shared {
    fn generate(&self, role: &Role) -> Result<Key, Error> {
        let _ = role;
        match read(&self.path) {
            Ok(key) => return Ok(key),
            Err(Fault::Missing) => {}
            Err(Fault::Broken(message)) => return Err(Error::generator(message)),
        }
        let key = random()?;
        match create(&self.path, &key) {
            Ok(()) => Ok(key),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                read(&self.path).map_err(|fault| Error::generator(fault.message(&self.path)))
            }
            Err(error) => Err(Error::generator(format!(
                "cannot create shared key {}: {error}",
                self.path.display()
            ))),
        }
    }
}

enum Fault {
    Broken(String),
    Missing,
}

impl Fault {
    fn message(self, path: &Path) -> String {
        match self {
            Self::Broken(message) => message,
            Self::Missing => format!("shared key disappeared: {}", path.display()),
        }
    }
}

fn read(path: &Path) -> Result<Key, Fault> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Fault::Missing);
        }
        Err(error) => {
            return Err(Fault::Broken(format!(
                "cannot read shared key {}: {error}",
                path.display()
            )));
        }
    };
    let record: Record = serde_json::from_str(text.trim()).map_err(|error| {
        Fault::Broken(format!("invalid shared key {}: {error}", path.display()))
    })?;
    if record.version != 1 {
        return Err(Fault::Broken(format!(
            "unsupported shared key version {} in {}",
            record.version,
            path.display()
        )));
    }
    Key::new(record.key)
        .map_err(|error| Fault::Broken(format!("invalid shared key {}: {error}", path.display())))
}

fn create(path: &Path, key: &Key) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    serde_json::to_writer(
        &mut file,
        &Record {
            version: 1,
            key: key.text().into(),
        },
    )
    .map_err(std::io::Error::other)?;
    file.write_all(b"\n")?;
    file.sync_all()
}

pub(crate) fn roles(
    specs: &BTreeMap<Role, Spec>,
) -> Result<BTreeMap<Role, Arc<dyn Generate>>, Error> {
    specs
        .iter()
        .map(|(role, spec)| Ok((role.clone(), build(spec)?)))
        .collect()
}
