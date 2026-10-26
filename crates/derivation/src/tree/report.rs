use super::forest::{Grove, Links, Trace};
use super::{Held, SCHEMA};
use locus::Atom;
use serde::Serialize;
use std::collections::BTreeMap;

pub struct Grown {
    pub grove: Grove,
    pub atoms: Vec<Held>,
    records: u64,
    from: u64,
    until: u64,
}

impl Grown {
    pub(super) fn new(grove: Grove, traces: &BTreeMap<String, Trace>) -> Self {
        let members = grove.members.iter().filter_map(|name| traces.get(name));
        let (mut records, mut from, mut until) = (0, u64::MAX, 0);
        for trace in members {
            records += trace.records;
            from = from.min(trace.from);
            until = until.max(trace.until);
        }
        Self {
            grove,
            atoms: Vec::new(),
            records,
            from,
            until,
        }
    }
}

pub struct Report {
    trees: Vec<Grown>,
    records: u64,
    untraced: u64,
    retained: Option<u64>,
}

#[derive(Serialize)]
pub struct Branch<'a> {
    schema: &'static str,
    kind: &'static str,
    root: &'a str,
    traces: usize,
    records: u64,
    seeds: usize,
    shown: usize,
    from: u64,
    until: u64,
    detached: bool,
    links: Links,
}

#[derive(Serialize)]
pub struct Leaf<'a> {
    schema: &'static str,
    kind: &'static str,
    root: &'a str,
    seed: bool,
    shown: bool,
    atom: &'a Atom,
}

#[derive(Serialize)]
pub struct Summary {
    schema: &'static str,
    kind: &'static str,
    records: u64,
    seeds: usize,
    untraced: u64,
    trees: usize,
    shown: usize,
    links: Links,
    #[serde(skip_serializing_if = "Option::is_none")]
    retained: Option<u64>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Record<'a> {
    Tree(Branch<'a>),
    Atom(Leaf<'a>),
    Summary(Summary),
}

impl Report {
    pub(super) fn new(trees: Vec<Grown>, records: u64, untraced: u64) -> Self {
        let mut trees = trees;
        trees.sort_by(|left, right| {
            (left.from, &left.grove.root).cmp(&(right.from, &right.grove.root))
        });
        Self {
            trees,
            records,
            untraced,
            retained: None,
        }
    }

    pub fn retained(mut self, retained: Option<u64>) -> Self {
        self.retained = retained;
        self
    }

    pub fn records(&self) -> Vec<Record<'_>> {
        let mut records = Vec::new();
        let mut links = Links::default();
        let (mut seeds, mut shown) = (0, 0);
        for tree in &self.trees {
            let branch = branch(tree);
            seeds += branch.seeds;
            shown += branch.shown;
            links.absorb(tree.grove.links);
            records.push(Record::Tree(branch));
            records.extend(tree.atoms.iter().map(|held| {
                Record::Atom(Leaf {
                    schema: SCHEMA,
                    kind: "atom",
                    root: &tree.grove.root,
                    seed: held.seed,
                    shown: held.shown,
                    atom: &held.atom,
                })
            }));
        }
        records.push(Record::Summary(Summary {
            schema: SCHEMA,
            kind: "summary",
            records: self.records,
            seeds,
            untraced: self.untraced,
            trees: self.trees.len(),
            shown,
            links,
            retained: self.retained,
        }));
        records
    }
}

fn branch(tree: &Grown) -> Branch<'_> {
    Branch {
        schema: SCHEMA,
        kind: "tree",
        root: &tree.grove.root,
        traces: tree.grove.members.len(),
        records: tree.records,
        seeds: tree.atoms.iter().filter(|held| held.seed).count(),
        shown: tree.atoms.iter().filter(|held| held.shown).count(),
        from: tree.from,
        until: tree.until,
        detached: tree.grove.detached,
        links: tree.grove.links,
    }
}
