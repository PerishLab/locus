mod condition;
mod forest;
mod report;

pub use condition::Condition;
pub use forest::Links;
pub use report::{Record, Report};

use forest::{Forest, Owner, Trace};
use locus::Atom;
use std::collections::BTreeMap;

pub const SCHEMA: &str = "locus.tree/v1";

struct Limits {
    trees: u64,
    atoms: u64,
    keys: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            trees: 1_000,
            atoms: 100_000,
            keys: 2_000_000,
        }
    }
}

pub struct Held {
    pub ordinal: u64,
    pub seed: bool,
    pub shown: bool,
    pub atom: Atom,
}

pub struct Tree {
    seeds: Vec<Condition>,
    show: Vec<Condition>,
    limits: Limits,
    traces: BTreeMap<String, Trace>,
    spans: BTreeMap<String, Owner>,
    held: BTreeMap<String, Vec<Held>>,
    count: u64,
    records: u64,
    untraced: u64,
}

impl Tree {
    pub fn new(seeds: Vec<Condition>, show: Vec<Condition>) -> Result<Self, String> {
        if seeds.is_empty() {
            return Err("tree requires at least one seed condition".into());
        }
        Ok(Self {
            seeds,
            show,
            limits: Limits::default(),
            traces: BTreeMap::new(),
            spans: BTreeMap::new(),
            held: BTreeMap::new(),
            count: 0,
            records: 0,
            untraced: 0,
        })
    }

    pub fn trees(mut self, trees: u64) -> Self {
        self.limits.trees = trees;
        self
    }

    pub fn atoms(mut self, atoms: u64) -> Self {
        self.limits.atoms = atoms;
        self
    }

    pub fn keys(mut self, keys: u64) -> Self {
        self.limits.keys = keys;
        self
    }

    pub fn observe(&mut self, atom: Atom) -> Result<(), String> {
        let ordinal = self.records;
        self.records = ordinal
            .checked_add(1)
            .ok_or_else(|| "Atom count overflowed".to_string())?;
        let seed = condition::all(&self.seeds, &atom);
        let Some(trace) = atom.context().get("locus.trace").cloned() else {
            self.untraced += u64::from(seed);
            return Ok(());
        };
        self.track(&trace, &atom)?;
        let shown = condition::all(&self.show, &atom);
        if seed || shown {
            self.count += 1;
            if self.count > self.limits.atoms {
                return Err(format!(
                    "tree holds more than {} seed or shown Atoms",
                    self.limits.atoms
                ));
            }
            let held = Held {
                ordinal,
                seed,
                shown,
                atom,
            };
            self.held.entry(trace).or_default().push(held);
        }
        Ok(())
    }

    fn track(&mut self, trace: &str, atom: &Atom) -> Result<(), String> {
        if !self.traces.contains_key(trace) {
            self.admit()?;
            let entry = Trace {
                from: atom.at(),
                until: atom.at(),
                ..Trace::default()
            };
            self.traces.insert(trace.to_string(), entry);
        }
        let entry = self.traces.get_mut(trace).ok_or("trace vanished")?;
        entry.records += 1;
        entry.from = entry.from.min(atom.at());
        entry.until = entry.until.max(atom.at());
        if let Some(parent) = atom.context().get("locus.parent") {
            match &entry.parent {
                Some(known) => entry.conflicting |= known != parent,
                None => entry.parent = Some(parent.clone()),
            }
        }
        let Some(span) = atom.context().get("locus.span") else {
            return Ok(());
        };
        let owner = match self.spans.get(span) {
            None => Owner::One(trace.to_string()),
            Some(Owner::One(owner)) if owner != trace => Owner::Many,
            Some(_) => return Ok(()),
        };
        if !self.spans.contains_key(span) {
            self.admit()?;
        }
        self.spans.insert(span.clone(), owner);
        Ok(())
    }

    fn admit(&self) -> Result<(), String> {
        let keys = (self.traces.len() + self.spans.len()) as u64;
        if keys >= self.limits.keys {
            return Err(format!(
                "tree tracks more than {} trace and span keys",
                self.limits.keys
            ));
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<Report, String> {
        let forest = Forest::new(&self.traces, &self.spans);
        let mut groves: BTreeMap<String, report::Grown> = BTreeMap::new();
        let seeded: Vec<&String> = self
            .held
            .iter()
            .filter(|(_, held)| held.iter().any(|held| held.seed))
            .map(|(trace, _)| trace)
            .collect();
        for trace in seeded {
            if groves
                .values()
                .any(|grown| grown.grove.members.contains(trace))
            {
                continue;
            }
            let grove = forest.grove(trace);
            if groves.len() as u64 >= self.limits.trees {
                return Err(format!(
                    "tree selects more than {} trees",
                    self.limits.trees
                ));
            }
            let grown = report::Grown::new(grove, &self.traces);
            groves.insert(grown.grove.root.clone(), grown);
        }
        for grown in groves.values_mut() {
            for member in &grown.grove.members {
                grown
                    .atoms
                    .extend(self.held.remove(member).unwrap_or_default());
            }
            grown.atoms.sort_by_key(|held| held.ordinal);
        }
        Ok(Report::new(
            groves.into_values().collect(),
            self.records,
            self.untraced,
        ))
    }
}
