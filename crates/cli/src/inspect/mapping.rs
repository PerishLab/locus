use super::model::{Group, Measurement};
use locus::{Atom, Edge, Role};
use serde_json::Value;
use std::collections::BTreeMap;

const FLOOR: u64 = 5;
const MINIMUM: usize = 8;
const MAXIMUM: usize = 64;

pub enum Mapping {
    Bytes { group: Role },
    Prefix { group: Role },
}

pub struct Observation {
    pub group: Group,
    pub measurement: Measurement,
}

pub(super) enum State {
    Bytes {
        role: Role,
        groups: BTreeMap<String, u64>,
    },
    Prefix {
        role: Role,
        groups: BTreeMap<String, Prefix>,
    },
}

#[derive(Default)]
pub(super) struct Prefix {
    atoms: u64,
    prefixes: BTreeMap<Vec<u8>, u64>,
}

impl State {
    pub fn new(mapping: &Mapping) -> Self {
        match mapping {
            Mapping::Bytes { group } => Self::Bytes {
                role: group.clone(),
                groups: BTreeMap::new(),
            },
            Mapping::Prefix { group } => Self::Prefix {
                role: group.clone(),
                groups: BTreeMap::new(),
            },
        }
    }

    pub fn observe(&mut self, atom: &Atom, encoded: u64) -> Result<(), String> {
        match self {
            Self::Bytes { role, groups } => {
                let Some(key) = atom.context().get(role.text()) else {
                    return Ok(());
                };
                let value = groups.entry(key.clone()).or_default();
                *value = value
                    .checked_add(encoded)
                    .ok_or_else(|| "representation byte count overflowed".to_string())?;
            }
            Self::Prefix { role, groups } => {
                let Some(key) = atom.context().get(role.text()) else {
                    return Ok(());
                };
                groups.entry(key.clone()).or_default().observe(atom)?;
            }
        }
        Ok(())
    }

    pub fn finish(self) -> Vec<Observation> {
        match self {
            Self::Bytes { role, groups } => groups
                .into_iter()
                .map(|(key, value)| Observation {
                    group: Group::new(&role, key),
                    measurement: Measurement::Bytes { value },
                })
                .collect(),
            Self::Prefix { role, groups } => groups
                .into_iter()
                .filter_map(|(key, prefix)| {
                    prefix.measurement().map(|measurement| Observation {
                        group: Group::new(&role, key),
                        measurement,
                    })
                })
                .collect(),
        }
    }
}

impl Prefix {
    fn observe(&mut self, atom: &Atom) -> Result<(), String> {
        self.atoms = self
            .atoms
            .checked_add(1)
            .ok_or_else(|| "group Atom count overflowed".to_string())?;
        let Some(content) = content(atom) else {
            return Ok(());
        };
        let ceiling = content.len().min(MAXIMUM);
        for length in MINIMUM..=ceiling {
            let count = self.prefixes.entry(content[..length].to_vec()).or_default();
            *count = count
                .checked_add(1)
                .ok_or_else(|| "prefix count overflowed".to_string())?;
        }
        Ok(())
    }

    fn measurement(self) -> Option<Measurement> {
        if self.atoms < FLOOR {
            return None;
        }
        self.prefixes
            .into_iter()
            .map(|(prefix, count)| (count, prefix.len()))
            .max_by_key(|(count, length)| (*count, *length))
            .map(|(repeated, bytes)| Measurement::Prefix {
                value: ((u128::from(repeated) * 100) / u128::from(self.atoms)) as u64,
                atoms: self.atoms,
                repeated,
                bytes,
            })
    }
}

fn content(atom: &Atom) -> Option<Vec<u8>> {
    let mut content = Vec::new();
    if let Some(source) = atom.source() {
        append(source.file(), &mut content);
        append(source.module(), &mut content);
        if let Some(function) = source.function() {
            append(function, &mut content);
        }
        if let Some(edge) = source.edge() {
            append(
                match edge {
                    Edge::Enter => "enter",
                    Edge::Return => "return",
                },
                &mut content,
            );
        }
        append(&source.line().to_string(), &mut content);
        append(&source.column().to_string(), &mut content);
    }
    if let Some(payload) = atom.payload() {
        flatten(payload, &mut content);
    }
    (!content.is_empty()).then_some(content)
}

fn flatten(value: &Value, content: &mut Vec<u8>) {
    if content.len() == MAXIMUM {
        return;
    }
    match value {
        Value::Null => append("null", content),
        Value::Bool(value) => append(&value.to_string(), content),
        Value::Number(value) => append(&value.to_string(), content),
        Value::String(value) => append(value, content),
        Value::Array(array) => array.iter().for_each(|value| flatten(value, content)),
        Value::Object(object) => object.values().for_each(|value| flatten(value, content)),
    }
}

fn append(value: &str, content: &mut Vec<u8>) {
    if content.len() == MAXIMUM {
        return;
    }
    if !content.is_empty() {
        content.push(0x1f);
    }
    let remaining = MAXIMUM - content.len();
    let value = value.as_bytes();
    content.extend_from_slice(&value[..value.len().min(remaining)]);
}
