use crate::input;
use locus::{Atom, Key, Role};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::BufRead;

const SCHEMA: &str = "locus.query/v1";

pub struct Selector {
    role: Role,
    key: Option<Key>,
}

impl Selector {
    pub fn new(role: String, key: Option<String>) -> Result<Self, String> {
        Ok(Self {
            role: Role::new(role).map_err(|error| error.to_string())?,
            key: key
                .map(Key::new)
                .transpose()
                .map_err(|error| error.to_string())?,
        })
    }
}

enum Projection {
    Identities(BTreeMap<String, u64>),
    Atoms(Vec<Atom>),
}

pub struct Report {
    selector: Selector,
    projection: Projection,
    records: u64,
    matched: u64,
}

impl Report {
    pub fn records(&self) -> Vec<Record<'_>> {
        let mut records: Vec<Record<'_>> = match &self.projection {
            Projection::Identities(identities) => identities
                .iter()
                .map(|(key, records)| {
                    Record::Identity(Identity {
                        schema: SCHEMA,
                        kind: "identity",
                        role: self.selector.role.text(),
                        key,
                        records: *records,
                    })
                })
                .collect(),
            Projection::Atoms(atoms) => atoms
                .iter()
                .map(|atom| {
                    Record::Atom(Match {
                        schema: SCHEMA,
                        kind: "atom",
                        role: self.selector.role.text(),
                        key: self.selector.key.as_ref().expect("atom query key").text(),
                        atom,
                    })
                })
                .collect(),
        };
        records.push(Record::Summary(Summary {
            schema: SCHEMA,
            kind: "summary",
            role: self.selector.role.text(),
            key: self.selector.key.as_ref().map(Key::text),
            records: self.records,
            matched: self.matched,
            identities: self.identities(),
        }));
        records
    }

    fn identities(&self) -> usize {
        match &self.projection {
            Projection::Identities(identities) => identities.len(),
            Projection::Atoms(atoms) => usize::from(!atoms.is_empty()),
        }
    }
}

pub fn scan(reader: impl BufRead, selector: Selector) -> Result<Report, String> {
    let mut projection = match selector.key {
        Some(_) => Projection::Atoms(Vec::new()),
        None => Projection::Identities(BTreeMap::new()),
    };
    let mut matched = 0_u64;
    let records = input::scan(reader, |atom, _| {
        let Some(key) = atom.context().get(selector.role.text()) else {
            return Ok(());
        };
        match &mut projection {
            Projection::Identities(identities) => {
                let records = identities.entry(key.clone()).or_default();
                *records = records
                    .checked_add(1)
                    .ok_or_else(|| "identity record count overflowed".to_string())?;
                matched = matched
                    .checked_add(1)
                    .ok_or_else(|| "matched Atom count overflowed".to_string())?;
            }
            Projection::Atoms(atoms)
                if selector.key.as_ref().is_some_and(|seen| seen.text() == key) =>
            {
                atoms.push(atom);
                matched = matched
                    .checked_add(1)
                    .ok_or_else(|| "matched Atom count overflowed".to_string())?;
            }
            Projection::Atoms(_) => {}
        }
        Ok(())
    })?;
    Ok(Report {
        selector,
        projection,
        records,
        matched,
    })
}

#[derive(Serialize)]
pub struct Identity<'a> {
    schema: &'static str,
    kind: &'static str,
    role: &'a str,
    key: &'a str,
    records: u64,
}

#[derive(Serialize)]
pub struct Match<'a> {
    schema: &'static str,
    kind: &'static str,
    role: &'a str,
    key: &'a str,
    atom: &'a Atom,
}

#[derive(Serialize)]
pub struct Summary<'a> {
    schema: &'static str,
    kind: &'static str,
    role: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<&'a str>,
    records: u64,
    matched: u64,
    identities: usize,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Record<'a> {
    Identity(Identity<'a>),
    Atom(Match<'a>),
    Summary(Summary<'a>),
}
