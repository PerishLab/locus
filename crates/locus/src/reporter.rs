use crate::{Atom, Error};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

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

    pub fn file(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self::new("file", json!({"path": path.to_string_lossy()}))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn options(&self) -> &Value {
        &self.options
    }
}

pub(crate) trait Report: Send + Sync {
    fn report(&self, atom: &Atom) -> Result<(), String>;
}

type Factory = fn(&Value) -> Result<Box<dyn Report>, Error>;

struct Registry {
    factories: std::collections::BTreeMap<&'static str, Factory>,
}

impl Registry {
    fn builtins() -> Self {
        let mut registry = Self {
            factories: std::collections::BTreeMap::new(),
        };
        register(&mut registry);
        registry
    }

    fn add(&mut self, name: &'static str, factory: Factory) {
        self.factories.insert(name, factory);
    }

    fn build(&self, spec: &Spec) -> Result<Box<dyn Report>, Error> {
        let factory = self
            .factories
            .get(spec.name())
            .ok_or_else(|| Error::config(format!("unknown reporter: {}", spec.name())))?;
        factory(spec.options())
    }
}

pub(crate) fn build(spec: &Spec) -> Result<Box<dyn Report>, Error> {
    Registry::builtins().build(spec)
}

fn register(registry: &mut Registry) {
    registry.add("file", file);
}

fn file(options: &Value) -> Result<Box<dyn Report>, Error> {
    let object = options
        .as_object()
        .ok_or_else(|| Error::config("file reporter options must be an object"))?;
    if object.len() != 1 || !object.contains_key("path") {
        return Err(Error::config(
            "file reporter options do not match its schema",
        ));
    }
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::config("file reporter requires path"))?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| Error::reporter(format!("cannot open report file {path}: {error}")))?;
    Ok(Box::new(Jsonl {
        file: Mutex::new(file),
    }))
}

struct Jsonl {
    file: Mutex<File>,
}

impl Report for Jsonl {
    fn report(&self, atom: &Atom) -> Result<(), String> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| "file reporter lock is poisoned".to_string())?;
        serde_json::to_writer(&mut *file, atom)
            .map_err(|error| format!("cannot encode Atom: {error}"))?;
        file.write_all(b"\n")
            .and_then(|_| file.flush())
            .map_err(|error| format!("cannot append Atom: {error}"))
    }
}
