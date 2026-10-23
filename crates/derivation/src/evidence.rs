use crate::inspect::Finding;
use locus::{Atom, Edge, Source};
use serde::Serialize;
use std::collections::BTreeMap;

pub const GROUPS: usize = 10_000;
pub const REFERENCES: usize = 4;
pub const CONTRIBUTIONS: usize = 32;
pub const BYTES: usize = 16 * 1024 * 1024;

#[derive(Serialize)]
pub struct Group {
    schema: &'static str,
    kind: &'static str,
    role: String,
    key: String,
    records: u64,
    bytes: u64,
    references: Vec<Reference>,
    contributions: Vec<Contribution>,
    omitted: Omitted,
}

#[derive(Default, Serialize)]
struct Omitted {
    references: u64,
    contributions: u64,
}

#[derive(Serialize)]
struct Reference {
    ordinal: u64,
    id: Option<String>,
    at: u64,
    producer: Option<String>,
    context: BTreeMap<String, String>,
    source: Option<Source>,
}

#[derive(Serialize)]
struct Contribution {
    producer: Option<String>,
    source: Option<Source>,
    records: u64,
    bytes: u64,
    enters: u64,
    returns: u64,
}

#[derive(Default)]
struct Budget {
    used: usize,
}

impl Budget {
    fn admit(&mut self, value: &impl Serialize) -> Result<bool, String> {
        let size = serde_json::to_vec(value)
            .map_err(|error| format!("cannot encode evidence: {error}"))?
            .len();
        if size > BYTES.saturating_sub(self.used) {
            return Ok(false);
        }
        self.used += size;
        Ok(true)
    }
}

type Groups = BTreeMap<String, Group>;

pub struct Evidence {
    groups: BTreeMap<String, Groups>,
    budget: Budget,
    records: u64,
}

impl Evidence {
    pub fn new(findings: &[Finding]) -> Result<Self, String> {
        let mut groups: BTreeMap<String, Groups> = BTreeMap::new();
        let mut budget = Budget::default();
        let mut count = 0;
        for finding in findings {
            let (_, role, key) = finding.identity();
            let keys = groups.entry(role.into()).or_default();
            if keys.contains_key(key) {
                continue;
            }
            count += 1;
            if count > GROUPS || !budget.admit(&(role, key))? {
                return Err("finding evidence exceeds group or identity byte limit".into());
            }
            keys.insert(key.into(), Group::new(role, key));
        }
        Ok(Self {
            groups,
            budget,
            records: 0,
        })
    }

    pub fn observe(&mut self, atom: &Atom, encoded: u64) -> Result<(), String> {
        self.records = self
            .records
            .checked_add(1)
            .ok_or("evidence count overflowed")?;
        for (role, keys) in &mut self.groups {
            let Some(group) = atom.context().get(role).and_then(|key| keys.get_mut(key)) else {
                continue;
            };
            group.observe(atom, encoded, self.records, &mut self.budget)?;
        }
        Ok(())
    }

    pub fn finish(self) -> impl Iterator<Item = Group> {
        self.groups.into_values().flat_map(BTreeMap::into_values)
    }
}

impl Group {
    fn new(role: &str, key: &str) -> Self {
        Self {
            schema: "locus.evidence/v1",
            kind: "group",
            role: role.into(),
            key: key.into(),
            records: 0,
            bytes: 0,
            references: Vec::new(),
            contributions: Vec::new(),
            omitted: Omitted::default(),
        }
    }

    fn observe(
        &mut self,
        atom: &Atom,
        encoded: u64,
        ordinal: u64,
        budget: &mut Budget,
    ) -> Result<(), String> {
        self.records = self
            .records
            .checked_add(1)
            .ok_or("group count overflowed")?;
        self.bytes = self
            .bytes
            .checked_add(encoded)
            .ok_or("group bytes overflowed")?;
        self.reference(atom, ordinal, budget)?;
        if let Some(contribution) = self.contributions.iter_mut().find(|entry| {
            entry.producer.as_deref() == atom.producer() && entry.source.as_ref() == atom.source()
        }) {
            contribution.observe(atom, encoded)?;
        } else {
            self.contribute(atom, encoded, budget)?;
        }
        Ok(())
    }

    fn reference(&mut self, atom: &Atom, ordinal: u64, budget: &mut Budget) -> Result<(), String> {
        if self.references.len() < REFERENCES {
            let reference = Reference {
                ordinal,
                id: atom.id().map(str::to_owned),
                at: atom.at(),
                producer: atom.producer().map(str::to_owned),
                context: atom.context().clone(),
                source: atom.source().cloned(),
            };
            if budget.admit(&reference)? {
                self.references.push(reference);
                return Ok(());
            }
        }
        self.omitted.references += 1;
        Ok(())
    }

    fn contribute(&mut self, atom: &Atom, encoded: u64, budget: &mut Budget) -> Result<(), String> {
        if self.contributions.len() < CONTRIBUTIONS {
            let mut contribution = Contribution {
                producer: atom.producer().map(str::to_owned),
                source: atom.source().cloned(),
                records: 0,
                bytes: 0,
                enters: 0,
                returns: 0,
            };
            contribution.observe(atom, encoded)?;
            if budget.admit(&contribution)? {
                self.contributions.push(contribution);
                return Ok(());
            }
        }
        self.omitted.contributions += 1;
        Ok(())
    }
}

impl Contribution {
    fn observe(&mut self, atom: &Atom, encoded: u64) -> Result<(), String> {
        self.records = self
            .records
            .checked_add(1)
            .ok_or("contribution count overflowed")?;
        self.bytes = self
            .bytes
            .checked_add(encoded)
            .ok_or("contribution bytes overflowed")?;
        match atom.source().and_then(Source::edge) {
            Some(Edge::Enter) => self.enters += 1,
            Some(Edge::Return) => self.returns += 1,
            None => {}
        }
        Ok(())
    }
}
