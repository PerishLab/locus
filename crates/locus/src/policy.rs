use crate::hook::{Adaptor, Hook};
use crate::{Error, Role};
use crate::{collector, generator, reporter};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Policy {
    pub(crate) producer: Option<String>,
    pub(crate) collectors: BTreeMap<String, Vec<collector::Spec>>,
    pub(crate) enabled: bool,
    pub(crate) reporter: Option<reporter::Spec>,
    pub(crate) fallback: generator::Spec,
    pub(crate) generators: BTreeMap<Role, generator::Spec>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            producer: None,
            collectors: BTreeMap::new(),
            enabled: false,
            reporter: None,
            fallback: generator::Spec::random(),
            generators: BTreeMap::new(),
        }
    }
}

impl Policy {
    pub fn producer(mut self, name: impl Into<String>) -> Self {
        self.producer = Some(name.into());
        self
    }

    pub fn collector(mut self, binding: impl Into<String>, spec: collector::Spec) -> Self {
        self.collectors
            .entry(binding.into())
            .or_default()
            .push(spec);
        self
    }

    pub fn reporter(mut self, spec: reporter::Spec) -> Self {
        self.enabled = true;
        self.reporter = Some(spec);
        self
    }

    pub fn fallback(mut self, spec: generator::Spec) -> Self {
        self.fallback = spec;
        self
    }

    pub fn generator(mut self, role: Role, spec: generator::Spec) -> Self {
        self.generators.insert(role, spec);
        self
    }

    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }
}

pub struct Config {
    pub(crate) policy: Policy,
    pub(crate) hook: Arc<dyn Hook>,
}

impl Config {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            hook: Arc::new(Adaptor),
        }
    }

    pub fn hook(mut self, hook: Arc<dyn Hook>) -> Self {
        self.hook = hook;
        self
    }
}

pub(crate) fn admit(producer: &str) -> Result<(), Error> {
    let sized = !producer.is_empty() && producer.len() <= 64;
    if sized && !producer.starts_with(['.', '-']) && producer.bytes().all(allowed) {
        Ok(())
    } else {
        Err(Error::config(format!("invalid producer {producer:?}")))
    }
}

fn allowed(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-".contains(&byte)
}
