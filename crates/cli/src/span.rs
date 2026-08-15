use crate::derive::{Derived, Ledger, SCHEMA, Tangle, Unclosed};
use crate::input;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::BufRead;

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
}

impl Report {
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
        records.extend(self.derived.unclosed.iter().map(Record::Unclosed));
        records.extend(self.derived.tangles.iter().map(Record::Tangle));
        records.push(Record::Summary(Summary {
            schema: SCHEMA,
            kind: "summary",
            records: self.records,
            matched: self.derived.matched,
            declarations: self.declarations.len(),
            unclosed: self.derived.unclosed.len(),
            tangled: self.derived.tangles.len(),
            refused: self.derived.tangles.iter().map(|tangle| tangle.spans).sum(),
        }));
        records
    }
}

pub fn scan(reader: impl BufRead) -> Result<Report, String> {
    let mut ledger = Ledger::grouped(None);
    let records = input::scan(reader, |atom, _| ledger.observe(&atom))?;
    let derived = ledger.finish();
    let mut declarations: BTreeMap<String, Measure> = BTreeMap::new();
    for held in &derived.held {
        let measure = declarations.entry(held.declaration.clone()).or_default();
        measure.spans += 1;
        measure.inclusive += held.inclusive;
        measure.held += held.held;
    }
    Ok(Report {
        declarations,
        derived,
        records,
    })
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
    Unclosed(&'a Unclosed),
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
    tangled: usize,
    refused: u64,
}
