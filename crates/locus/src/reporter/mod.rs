mod api;
pub mod spool;

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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn options(&self) -> &Value {
        &self.options
    }
}

pub(crate) trait Report: Send + Sync {
    fn report(&self, atom: &Atom) -> Result<Option<String>, String>;

    fn target(&self) -> Option<Target> {
        None
    }
}

pub(crate) fn build(spec: &Spec) -> Result<Box<dyn Report>, Error> {
    match spec.name() {
        "api" => api::build(spec.options()),
        name => Err(Error::config(format!("unknown reporter: {name}"))),
    }
}
