use super::Interval;
use super::reach::Reach;
use locus::Atom;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Links {
    pub nested: u64,
    pub outlived: u64,
    pub missing: u64,
    pub conflicting: u64,
}

impl Links {
    pub fn absorb(&mut self, other: Self) {
        self.nested = self.nested.max(other.nested);
        self.outlived = self.outlived.max(other.outlived);
        self.missing = self.missing.max(other.missing);
        self.conflicting = self.conflicting.max(other.conflicting);
    }
}

pub(super) struct Claim {
    parent: String,
    conflicting: bool,
}

#[derive(Default)]
pub(super) struct Resolution {
    pub(super) covers: BTreeMap<(String, usize), Vec<(u64, u64)>>,
    pub(super) reaches: BTreeMap<String, Vec<Reach>>,
    pub(super) links: Links,
}

pub(super) fn claim(claims: &mut BTreeMap<String, Claim>, atom: &Atom) {
    let Some(parent) = atom.context().get("locus.parent") else {
        return;
    };
    let trace = atom
        .context()
        .get("locus.trace")
        .cloned()
        .unwrap_or_default();
    match claims.get_mut(&trace) {
        Some(claim) => claim.conflicting |= &claim.parent != parent,
        None => {
            claims.insert(
                trace,
                Claim {
                    parent: parent.clone(),
                    conflicting: false,
                },
            );
        }
    }
}

pub(super) fn resolve(
    traces: &BTreeMap<String, Vec<Interval>>,
    claims: &BTreeMap<String, Claim>,
) -> Resolution {
    let mut index = BTreeMap::new();
    for (trace, intervals) in traces {
        for (position, interval) in intervals.iter().enumerate() {
            if !interval.headless {
                index.insert(interval.span.as_str(), (trace.as_str(), position));
            }
        }
    }
    let mut resolution = Resolution::default();
    for (child, claim) in claims {
        if claim.conflicting {
            resolution.links.conflicting += 1;
            continue;
        }
        match index.get(claim.parent.as_str()) {
            Some(&(trace, position)) if trace != child => {
                let parent = &traces[trace][position];
                let intervals = traces.get(child).map(Vec::as_slice).unwrap_or_default();
                attach(&mut resolution, (trace, position), parent, intervals);
            }
            _ => resolution.links.missing += 1,
        }
    }
    resolution
}

fn attach(
    resolution: &mut Resolution,
    (trace, position): (&str, usize),
    parent: &Interval,
    intervals: &[Interval],
) {
    let at = intervals.iter().map(|interval| interval.at).min();
    let until = intervals.iter().map(|interval| interval.until).max();
    match (at, until) {
        (Some(at), Some(until)) if at < parent.at || until > parent.until => {
            resolution.links.outlived += 1;
            resolution
                .reaches
                .entry(trace.to_string())
                .or_default()
                .push(Reach {
                    at: at.min(parent.at),
                    until: until.max(parent.until),
                });
        }
        _ => {
            resolution.links.nested += 1;
            resolution
                .covers
                .entry((trace.to_string(), position))
                .or_default()
                .extend(
                    intervals
                        .iter()
                        .map(|interval| (interval.at, interval.until)),
                );
        }
    }
}
