use crate::Error;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Role(String);

impl Role {
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 {
            return Err(Error::context("role must contain 1..=128 bytes"));
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-:/".contains(&byte))
        {
            return Err(Error::context(format!("invalid role: {value}")));
        }
        Ok(Self(value))
    }

    pub fn trace() -> Self {
        Self("locus.trace".into())
    }

    pub fn span() -> Self {
        Self("locus.span".into())
    }

    pub fn text(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Role {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Key(String);

impl Key {
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if value.is_empty() || value.len() > 512 {
            return Err(Error::context("key must contain 1..=512 bytes"));
        }
        if value.chars().any(char::is_control) {
            return Err(Error::context("key cannot contain control characters"));
        }
        Ok(Self(value))
    }

    pub fn text(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Context {
    values: Arc<BTreeMap<Role, Key>>,
}

impl Context {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn load(entries: impl IntoIterator<Item = (Role, Key)>) -> Self {
        Self {
            values: Arc::new(entries.into_iter().collect()),
        }
    }

    pub fn read(&self, role: &Role) -> Option<&Key> {
        self.values.get(role)
    }

    pub fn entries(&self) -> impl Iterator<Item = (&Role, &Key)> {
        self.values.iter()
    }

    pub(crate) fn bind(&self, role: Role, key: Key) -> Self {
        if self.values.get(&role) == Some(&key) {
            return self.clone();
        }
        let mut values = self.values.as_ref().clone();
        values.insert(role, key);
        Self {
            values: Arc::new(values),
        }
    }

    pub(crate) fn snapshot(&self) -> BTreeMap<String, String> {
        self.values
            .iter()
            .map(|(role, key)| (role.text().into(), key.text().into()))
            .collect()
    }
}
