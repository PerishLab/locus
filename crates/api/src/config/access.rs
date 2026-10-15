use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct Readers {
    entries: Vec<Entry>,
}

#[derive(Clone, Debug)]
struct Entry {
    name: String,
    digest: String,
}

impl Readers {
    pub fn read(home: &Path) -> Result<Self, String> {
        let declared = crate::config::read(home)?;
        let mut readers = Self::default();
        for reader in declared.reader {
            readers = readers.with(&reader.name, &reader.digest)?;
        }
        Ok(readers)
    }

    pub fn with(mut self, name: &str, digest: &str) -> Result<Self, String> {
        let name = name.trim();
        let digest = digest.trim().to_ascii_lowercase();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        {
            return Err(format!(
                "reader name {name:?} must be ASCII letters, digits, '-', '_' or '.'"
            ));
        }
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!(
                "reader {name} digest must be 64 hexadecimal characters"
            ));
        }
        let names: BTreeSet<&str> = self
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        if names.contains(name) {
            return Err(format!("reader {name} is declared twice"));
        }
        if self.entries.iter().any(|entry| entry.digest == digest) {
            return Err(format!("reader {name} repeats another reader's digest"));
        }
        self.entries.push(Entry {
            name: name.to_string(),
            digest,
        });
        Ok(self)
    }

    pub fn open(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }

    pub fn admit(&self, token: &str) -> Option<&str> {
        if token.is_empty() {
            return None;
        }
        let digest = digest(token);
        self.entries
            .iter()
            .find(|entry| same(&entry.digest, &digest))
            .map(|entry| entry.name.as_str())
    }
}

pub fn digest(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn same(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
}
