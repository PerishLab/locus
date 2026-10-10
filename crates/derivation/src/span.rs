use crate::derive::{Derived, Ledger, Orphan, SCHEMA, Tangle};
use locus::Atom;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Default)]
struct Measure {
    spans: u64,
    inclusive: u128,
    held: u128,
}

pub struct Report {
    declarations: BTreeMap<String, Measure>,
    derived: Derived,
    records: u64,
    retained: Option<u64>,
}

impl Report {
    pub fn retained(mut self, retained: Option<u64>) -> Self {
        self.retained = retained;
        self
    }

    pub fn records(&self) -> Vec<Record<'_>> {
        let mut records: Vec<Record<'_>> = self
            .declarations
            .iter()
            .map(|(declaration, measure)| {
                Record::Declaration(Declaration {
                    schema: SCHEMA,
                    kind: "declaration",
                    declaration,
                    spans: measure.spans,
                    inclusive: measure.inclusive,
                    held: measure.held,
                })
            })
            .collect();
        records.extend(self.derived.unclosed.iter().map(Record::Orphan));
        records.extend(self.derived.headless.iter().map(Record::Orphan));
        records.extend(self.derived.tangles.iter().map(Record::Tangle));
        records.push(Record::Summary(Summary {
            schema: SCHEMA,
            kind: "summary",
            records: self.records,
            matched: self.derived.matched,
            declarations: self.declarations.len(),
            unclosed: self.derived.unclosed.len(),
            headless: self.derived.headless.len(),
            tangled: self.derived.tangles.len(),
            refused: self.derived.tangles.iter().map(|tangle| tangle.spans).sum(),
            retained: self.retained,
        }));
        records
    }
}

#[derive(Default)]
pub struct Span {
    ledger: Ledger,
    records: u64,
}

impl Span {
    pub fn observe(&mut self, atom: &Atom) -> Result<(), String> {
        self.records = self
            .records
            .checked_add(1)
            .ok_or_else(|| "Atom count overflowed".to_string())?;
        self.ledger.observe(atom)
    }

    pub fn finish(self) -> Report {
        let derived = self.ledger.finish();
        let mut declarations: BTreeMap<String, Measure> = BTreeMap::new();
        for held in &derived.held {
            let measure = declarations.entry(held.declaration.clone()).or_default();
            measure.spans += 1;
            measure.inclusive += held.inclusive;
            measure.held += held.held;
        }
        Report {
            declarations,
            derived,
            records: self.records,
            retained: None,
        }
    }
}

#[derive(Serialize)]
pub struct Declaration<'a> {
    schema: &'static str,
    kind: &'static str,
    declaration: &'a str,
    spans: u64,
    inclusive: u128,
    held: u128,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Record<'a> {
    Declaration(Declaration<'a>),
    Orphan(&'a Orphan),
    Tangle(&'a Tangle),
    Summary(Summary),
}

#[derive(Serialize)]
pub struct Summary {
    schema: &'static str,
    kind: &'static str,
    records: u64,
    matched: u64,
    declarations: usize,
    unclosed: usize,
    headless: usize,
    tangled: usize,
    refused: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    retained: Option<u64>,
}
