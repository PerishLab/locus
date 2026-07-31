use locus::{Atom, Edge};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::BufRead;

const BYTES: usize = 8 * 1024;
const DENSITY: usize = 80;
const FLOOR: usize = 5;
const MINIMUM: usize = 8;
const MAXIMUM: usize = 64;

#[derive(Debug, Serialize)]
#[serde(tag = "law")]
pub enum Finding {
    #[serde(rename = "trace.size")]
    Size {
        trace: String,
        bytes: usize,
        limit: usize,
    },
    #[serde(rename = "trace.prefix")]
    Prefix {
        trace: String,
        atoms: usize,
        repeated: usize,
        bytes: usize,
        limit: usize,
    },
}

#[derive(Default)]
struct Trace {
    bytes: usize,
    atoms: usize,
    prefixes: BTreeMap<Vec<u8>, usize>,
}

impl Trace {
    fn observe(&mut self, atom: &Atom, bytes: usize) -> Result<(), String> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| "trace byte count overflowed".to_string())?;
        self.atoms = self
            .atoms
            .checked_add(1)
            .ok_or_else(|| "trace Atom count overflowed".to_string())?;
        let Some(content) = content(atom) else {
            return Ok(());
        };
        let ceiling = content.len().min(MAXIMUM);
        for length in MINIMUM..=ceiling {
            *self.prefixes.entry(content[..length].to_vec()).or_default() += 1;
        }
        Ok(())
    }

    fn findings(&self, trace: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        if self.bytes > BYTES {
            findings.push(Finding::Size {
                trace: trace.into(),
                bytes: self.bytes,
                limit: BYTES,
            });
        }
        if let Some((repeated, bytes)) = self.repetition() {
            findings.push(Finding::Prefix {
                trace: trace.into(),
                atoms: self.atoms,
                repeated,
                bytes,
                limit: DENSITY,
            });
        }
        findings
    }

    fn repetition(&self) -> Option<(usize, usize)> {
        if self.atoms < FLOOR {
            return None;
        }
        self.prefixes
            .iter()
            .filter(|(_, count)| (**count as u128) * 100 > (self.atoms as u128) * (DENSITY as u128))
            .map(|(prefix, count)| (*count, prefix.len()))
            .max_by_key(|(count, length)| (*count, *length))
    }
}

pub fn scan(mut reader: impl BufRead) -> Result<Vec<Finding>, String> {
    let mut traces = BTreeMap::<String, Trace>::new();
    let mut offset = 0;
    loop {
        let mut line = Vec::new();
        let bytes = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("cannot read line {}: {error}", offset + 1))?;
        if bytes == 0 {
            break;
        }
        offset += 1;
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.is_empty() {
            return Err(format!("invalid Atom on line {offset}: empty record"));
        }
        let atom: Atom = serde_json::from_slice(&line)
            .map_err(|error| format!("invalid Atom on line {offset}: {error}"))?;
        let Some(trace) = atom.context().get("locus.trace") else {
            continue;
        };
        traces
            .entry(trace.clone())
            .or_default()
            .observe(&atom, bytes)?;
    }
    Ok(traces
        .iter()
        .flat_map(|(trace, state)| state.findings(trace))
        .collect())
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
    if content.is_empty() {
        return None;
    }
    Some(content)
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
        Value::Array(array) => {
            for value in array {
                flatten(value, content);
            }
        }
        Value::Object(object) => {
            for value in object.values() {
                flatten(value, content);
            }
        }
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
