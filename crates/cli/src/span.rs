use crate::input;
use locus::{Atom, Edge};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::BufRead;

const SCHEMA: &str = "locus.span/v1";

struct Interval {
    declaration: String,
    at: u64,
    until: u64,
}

#[derive(Default)]
struct Measure {
    spans: u64,
    inclusive: u128,
    held: u128,
}

pub struct Report {
    declarations: BTreeMap<String, Measure>,
    unclosed: Vec<Unclosed>,
    records: u64,
    matched: u64,
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
        records.extend(self.unclosed.iter().map(Record::Unclosed));
        records.push(Record::Summary(Summary {
            schema: SCHEMA,
            kind: "summary",
            records: self.records,
            matched: self.matched,
            declarations: self.declarations.len(),
            unclosed: self.unclosed.len(),
        }));
        records
    }
}

pub fn scan(reader: impl BufRead) -> Result<Report, String> {
    let mut open: BTreeMap<String, Frame> = BTreeMap::new();
    let mut traces: BTreeMap<String, Vec<Interval>> = BTreeMap::new();
    let mut matched = 0_u64;
    let records = input::scan(reader, |atom, _| {
        let Some(frame) = frame(&atom) else {
            return Ok(());
        };
        matched = matched
            .checked_add(1)
            .ok_or_else(|| "matched Atom count overflowed".to_string())?;
        match frame.edge {
            Edge::Enter => entered(&mut open, frame),
            Edge::Return => returned(&mut open, &mut traces, frame),
        }
    })?;
    let mut declarations: BTreeMap<String, Measure> = BTreeMap::new();
    for intervals in traces.values_mut() {
        intervals.sort_by_key(|interval| (interval.at, std::cmp::Reverse(interval.until)));
        nested(intervals)?;
        for (index, interval) in intervals.iter().enumerate() {
            let inclusive = u128::from(interval.until - interval.at);
            let measure = declarations
                .entry(interval.declaration.clone())
                .or_default();
            measure.spans += 1;
            measure.inclusive += inclusive;
            measure.held += inclusive - covered(intervals, index);
        }
    }
    let unclosed = open
        .into_values()
        .map(|frame| Unclosed {
            schema: SCHEMA,
            kind: "unclosed",
            declaration: frame.declaration,
            file: frame.file,
            trace: frame.trace,
            span: frame.span,
            at: frame.at,
        })
        .collect();
    Ok(Report {
        declarations,
        unclosed,
        records,
        matched,
    })
}

struct Frame {
    span: String,
    declaration: String,
    file: String,
    trace: String,
    edge: Edge,
    at: u64,
}

fn frame(atom: &Atom) -> Option<Frame> {
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
        edge,
        at: atom.at(),
    })
}

fn entered(open: &mut BTreeMap<String, Frame>, frame: Frame) -> Result<(), String> {
    if open.contains_key(&frame.span) {
        return Err(format!("span {} enters again before returning", frame.span));
    }
    open.insert(frame.span.clone(), frame);
    Ok(())
}

fn returned(
    open: &mut BTreeMap<String, Frame>,
    traces: &mut BTreeMap<String, Vec<Interval>>,
    frame: Frame,
) -> Result<(), String> {
    let Some(entered) = open.remove(&frame.span) else {
        return Err(format!("span {} returns without entering", frame.span));
    };
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
        at: entered.at,
        until: frame.at,
    });
    Ok(())
}

fn nested(intervals: &[Interval]) -> Result<(), String> {
    for (index, outer) in intervals.iter().enumerate() {
        for inner in &intervals[index + 1..] {
            if inner.at >= outer.until {
                break;
            }
            if inner.until > outer.until {
                return Err(format!(
                    "spans overlap without nesting in {} and {}; held time is underivable",
                    outer.declaration, inner.declaration
                ));
            }
        }
    }
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
pub struct Declaration<'a> {
    schema: &'static str,
    kind: &'static str,
    declaration: &'a str,
    spans: u64,
    inclusive: u128,
    held: u128,
}

#[derive(Serialize)]
pub struct Unclosed {
    schema: &'static str,
    kind: &'static str,
    declaration: String,
    file: String,
    trace: String,
    span: String,
    at: u64,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Record<'a> {
    Declaration(Declaration<'a>),
    Unclosed(&'a Unclosed),
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
}
