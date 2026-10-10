use locus::{Atom, Key, Role};
use serde::Serialize;
use std::collections::BTreeMap;

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
    query: Query,
    retained: Option<u64>,
}

impl Report {
    pub fn retained(mut self, retained: Option<u64>) -> Self {
        self.retained = retained;
        self
    }

    pub fn records(&self) -> Vec<Record<'_>> {
        let query = &self.query;
        let mut records: Vec<Record<'_>> = match &query.projection {
            Projection::Identities(identities) => identities
                .iter()
                .map(|(key, records)| {
                    Record::Identity(Identity {
                        schema: SCHEMA,
                        kind: "identity",
                        role: query.selector.role.text(),
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
                        role: query.selector.role.text(),
                        key: query.selector.key.as_ref().expect("atom query key").text(),
                        atom,
                    })
                })
                .collect(),
        };
        records.push(Record::Summary(Summary {
            schema: SCHEMA,
            kind: "summary",
            role: query.selector.role.text(),
            key: query.selector.key.as_ref().map(Key::text),
            records: query.records,
            matched: query.matched,
            identities: self.identities(),
            retained: self.retained,
        }));
        records
    }

    fn identities(&self) -> usize {
        match &self.query.projection {
            Projection::Identities(identities) => identities.len(),
            Projection::Atoms(atoms) => usize::from(!atoms.is_empty()),
        }
    }
}

pub struct Query {
    selector: Selector,
    projection: Projection,
    records: u64,
    matched: u64,
}

impl Query {
    pub fn new(selector: Selector) -> Self {
        let projection = match selector.key {
            Some(_) => Projection::Atoms(Vec::new()),
            None => Projection::Identities(BTreeMap::new()),
        };
        Self {
            selector,
            projection,
            records: 0,
            matched: 0,
        }
    }

    pub fn observe(&mut self, atom: Atom) -> Result<(), String> {
        self.records = self
            .records
            .checked_add(1)
            .ok_or_else(|| "Atom count overflowed".to_string())?;
        let Some(key) = atom.context().get(self.selector.role.text()) else {
            return Ok(());
        };
        match &mut self.projection {
            Projection::Identities(identities) => {
                let records = identities.entry(key.clone()).or_default();
                *records = records
                    .checked_add(1)
                    .ok_or_else(|| "identity record count overflowed".to_string())?;
            }
            Projection::Atoms(atoms)
                if self
                    .selector
                    .key
                    .as_ref()
                    .is_some_and(|seen| seen.text() == key) =>
            {
                atoms.push(atom);
            }
            Projection::Atoms(_) => return Ok(()),
        }
        self.matched = self
            .matched
            .checked_add(1)
            .ok_or_else(|| "matched Atom count overflowed".to_string())?;
        Ok(())
    }

    pub fn finish(self) -> Report {
        Report {
            query: self,
            retained: None,
        }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    retained: Option<u64>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Record<'a> {
    Identity(Identity<'a>),
    Atom(Match<'a>),
    Summary(Summary<'a>),
}
