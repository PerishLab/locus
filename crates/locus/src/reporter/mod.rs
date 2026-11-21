mod api;
pub mod delivery;
mod https;
mod launcher;
pub mod spool;
mod worker;

pub(crate) use api::{Target, enroll};

use crate::{Atom, Error};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

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

    pub fn api(endpoint: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self::new(
            "api",
            json!({"endpoint": endpoint.into(), "path": path.to_string_lossy()}),
        )
    }

    pub fn https(
        endpoint: impl Into<String>,
        credential: impl Into<PathBuf>,
        path: impl Into<PathBuf>,
    ) -> Self {
        let credential = credential.into();
        let path = path.into();
        Self::new(
            "https",
            json!({"endpoint":endpoint.into(), "key_file":credential.to_str(), "path":path.to_str()}),
        )
    }

    pub fn installed(self) -> Self {
        match launcher::installed() {
            Some(path) => self.sender(path),
            None => self,
        }
    }
    pub fn sender(mut self, path: impl Into<PathBuf>) -> Self {
        if let Some(options) = self.options.as_object_mut() {
            options.insert("sender".into(), json!(path.into().to_str()));
        }
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn options(&self) -> &Value {
        &self.options
    }
}

pub(crate) trait Report: Send + Sync {
    fn report(&self, atom: &Atom) -> Result<Option<String>, String>;

    fn settle(&self) -> Result<(), String> {
        Ok(())
    }

    fn target(&self) -> Option<Target> {
        None
    }
}

pub(crate) fn build(spec: &Spec, producer: Option<&str>) -> Result<Box<dyn Report>, Error> {
    match spec.name() {
        "api" => api::build(spec.options()),
        "https" => https::build(spec.options(), producer),
        name => Err(Error::config(format!("unknown reporter: {name}"))),
    }
}
