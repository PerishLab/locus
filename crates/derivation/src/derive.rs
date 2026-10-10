mod reach;

use locus::{Atom, Edge, Role};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "locus.span/v1";

pub struct Interval {
    pub declaration: String,
    pub group: Option<String>,
    pub at: u64,
    pub until: u64,
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
    open: BTreeMap<String, Frame>,
    traces: BTreeMap<String, Vec<Interval>>,
    headless: Vec<Frame>,
    head: Option<u64>,
    matched: u64,
}

pub struct Derived {
    pub held: Vec<Held>,
    pub unclosed: Vec<Orphan>,
    pub headless: Vec<Orphan>,
    pub tangles: Vec<Tangle>,
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
                    headless: true,
                });
        }
        for (trace, intervals) in self.traces.iter_mut() {
            intervals.sort_by_key(|interval| {
                (
                    interval.at,
                    !interval.headless,
                    std::cmp::Reverse(interval.until),
                )
            });
            let reaches = reach::tangled(intervals);
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
                held.push(Held {
                    declaration: interval.declaration.clone(),
                    group: interval.group.clone(),
                    inclusive,
                    held: inclusive - covered(intervals, index),
                });
            }
            tangles.extend(refused.into_values());
        }
        Derived {
            held,
            unclosed: self.open.into_values().map(Orphan::unclosed).collect(),
            headless: self.headless.into_iter().map(Orphan::headless).collect(),
            tangles,
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
        headless: false,
    });
    Ok(())
}

fn covered(intervals: &[Interval], index: usize) -> u128 {
    let outer = &intervals[index];
    let mut covered = 0_u128;
    let mut edge = outer.at;
    for inner in &intervals[index + 1..] {
        if inner.at >= outer.until {
            break;
        }
        if inner.at < edge {
            continue;
        }
        covered += u128::from(inner.until - inner.at);
        edge = inner.until;
    }
    covered
}

#[derive(Serialize)]
pub struct Orphan {
    schema: &'static str,
    kind: &'static str,
    declaration: String,
    file: String,
    trace: String,
    span: String,
    at: u64,
}

impl Orphan {
    fn unclosed(frame: Frame) -> Self {
        Self::name("unclosed", frame)
    }

    fn headless(frame: Frame) -> Self {
        Self::name("headless", frame)
    }

    fn name(kind: &'static str, frame: Frame) -> Self {
        Self {
            schema: SCHEMA,
            kind,
            declaration: frame.declaration,
            file: frame.file,
            trace: frame.trace,
            span: frame.span,
            at: frame.at,
        }
    }
}

#[derive(Serialize)]
pub struct Tangle {
    schema: &'static str,
    kind: &'static str,
    trace: String,
    at: u64,
    until: u64,
    pub spans: u64,
    declarations: BTreeSet<String>,
}

impl Tangle {
    fn open(trace: &str, reach: &reach::Reach) -> Self {
        Self {
            schema: SCHEMA,
            kind: "tangled",
            trace: trace.to_string(),
            at: reach.at,
            until: reach.until,
            spans: 0,
            declarations: BTreeSet::new(),
        }
    }

    fn mark(&mut self, declaration: &str) {
        self.spans += 1;
        self.declarations.insert(declaration.to_string());
    }
}
