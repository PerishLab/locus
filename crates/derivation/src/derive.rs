mod link;
pub mod metric;
mod reach;
mod report;
pub(crate) mod seen;
pub mod span;
pub use report::{Orphan, Tangle};

pub use link::Links;

use locus::{Atom, Edge, Role};
use seen::Seen;
use std::collections::BTreeMap;

pub const SCHEMA: &str = "locus.span/v1";

pub struct Interval {
    pub declaration: String,
    pub group: Option<String>,
    pub at: u64,
    pub until: u64,
    span: String,
    headless: bool,
}

pub struct Held {
    pub declaration: String,
    pub group: Option<String>,
    pub inclusive: u128,
    pub held: u128,
}

#[derive(Default)]
pub struct Ledger {
    group: Option<Role>,
    seen: Seen,
    open: BTreeMap<String, Frame>,
    traces: BTreeMap<String, Vec<Interval>>,
    claims: BTreeMap<String, link::Claim>,
    headless: Vec<Frame>,
    head: Option<u64>,
    matched: u64,
}

pub struct Derived {
    pub held: Vec<Held>,
    pub unclosed: Vec<Orphan>,
    pub headless: Vec<Orphan>,
    pub tangles: Vec<Tangle>,
    pub links: Links,
    pub matched: u64,
}

impl Ledger {
    pub fn grouped(group: Option<Role>) -> Self {
        Self {
            group,
            ..Self::default()
        }
    }

    pub fn observe(&mut self, atom: &Atom) -> Result<(), String> {
        if !self.seen.admit(atom)? {
            return Ok(());
        }
        link::claim(&mut self.claims, atom);
        let Some(frame) = self.frame(atom) else {
            return Ok(());
        };
        self.matched = self
            .matched
            .checked_add(1)
            .ok_or_else(|| "matched Atom count overflowed".to_string())?;
        self.head = Some(self.head.map_or(frame.at, |head| head.min(frame.at)));
        match frame.edge {
            Edge::Enter => entered(&mut self.open, frame),
            Edge::Return => match self.open.remove(&frame.span) {
                Some(entered) => returned(&mut self.traces, entered, frame),
                None => {
                    self.headless.push(frame);
                    Ok(())
                }
            },
        }
    }

    pub fn finish(mut self) -> Derived {
        let mut held = Vec::new();
        let mut tangles = Vec::new();
        let head = self.head.unwrap_or_default();
        for frame in &self.headless {
            self.traces
                .entry(frame.trace.clone())
                .or_default()
                .push(Interval {
                    declaration: frame.declaration.clone(),
                    group: frame.group.clone(),
                    at: head,
                    until: frame.at,
                    span: frame.span.clone(),
                    headless: true,
                });
        }
        for intervals in self.traces.values_mut() {
            intervals.sort_by_key(|interval| {
                (
                    interval.at,
                    !interval.headless,
                    std::cmp::Reverse(interval.until),
                )
            });
        }
        let mut resolution = link::resolve(&self.traces, &self.claims);
        for (trace, intervals) in &self.traces {
            let mut reaches = reach::tangled(intervals);
            reaches.extend(resolution.reaches.remove(trace).unwrap_or_default());
            let reaches = reach::merge(reaches);
            let mut refused: BTreeMap<usize, Tangle> = BTreeMap::new();
            for (index, interval) in intervals.iter().enumerate() {
                if interval.headless {
                    continue;
                }
                if let Some(at) = reach::struck(&reaches, interval) {
                    refused
                        .entry(at)
                        .or_insert_with(|| Tangle::open(trace, &reaches[at]))
                        .mark(&interval.declaration);
                    continue;
                }
                let inclusive = u128::from(interval.until - interval.at);
                let linked = resolution
                    .covers
                    .remove(&(trace.clone(), index))
                    .unwrap_or_default();
                held.push(Held {
                    declaration: interval.declaration.clone(),
                    group: interval.group.clone(),
                    inclusive,
                    held: inclusive - covered(intervals, index, linked),
                });
            }
            tangles.extend(refused.into_values());
        }
        Derived {
            held,
            unclosed: self.open.into_values().map(Orphan::unclosed).collect(),
            headless: self.headless.into_iter().map(Orphan::headless).collect(),
            tangles,
            links: resolution.links,
            matched: self.matched,
        }
    }

    fn frame(&self, atom: &Atom) -> Option<Frame> {
        let source = atom.source()?;
        let function = source.function()?;
        let edge = source.edge()?.clone();
        let span = atom.context().get("locus.span")?.clone();
        Some(Frame {
            span,
            declaration: format!("{}::{function}", source.module()),
            file: source.file().to_string(),
            trace: atom
                .context()
                .get("locus.trace")
                .cloned()
                .unwrap_or_default(),
            group: self
                .group
                .as_ref()
                .and_then(|role| atom.context().get(role.text()).cloned()),
            edge,
            at: atom.at(),
        })
    }
}

struct Frame {
    span: String,
    declaration: String,
    file: String,
    trace: String,
    group: Option<String>,
    edge: Edge,
    at: u64,
}

fn entered(open: &mut BTreeMap<String, Frame>, frame: Frame) -> Result<(), String> {
    if open.contains_key(&frame.span) {
        return Err(format!("span {} enters again before returning", frame.span));
    }
    open.insert(frame.span.clone(), frame);
    Ok(())
}

fn returned(
    traces: &mut BTreeMap<String, Vec<Interval>>,
    entered: Frame,
    frame: Frame,
) -> Result<(), String> {
    if entered.declaration != frame.declaration {
        return Err(format!(
            "span {} returns from another declaration",
            frame.span
        ));
    }
    if frame.at < entered.at {
        return Err(format!("span {} returns before it enters", frame.span));
    }
    traces.entry(entered.trace).or_default().push(Interval {
        declaration: entered.declaration,
        group: entered.group,
        at: entered.at,
        until: frame.at,
        span: entered.span,
        headless: false,
    });
    Ok(())
}

fn covered(intervals: &[Interval], index: usize, mut ranges: Vec<(u64, u64)>) -> u128 {
    let outer = &intervals[index];
    let mut edge = outer.at;
    for inner in &intervals[index + 1..] {
        if inner.at >= outer.until {
            break;
        }
        if inner.at < edge {
            continue;
        }
        ranges.push((inner.at, inner.until));
        edge = inner.until;
    }
    ranges.sort_unstable();
    let mut covered = 0_u128;
    let mut edge = outer.at;
    for (at, until) in ranges {
        let at = at.max(edge);
        let until = until.min(outer.until);
        if until > at {
            covered += u128::from(until - at);
            edge = until;
        }
    }
    covered
}
