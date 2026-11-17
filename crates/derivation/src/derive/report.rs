use super::{Frame, SCHEMA, reach};
use serde::Serialize;
use std::collections::BTreeSet;

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
    pub(super) fn unclosed(frame: Frame) -> Self {
        Self::name("unclosed", frame)
    }

    pub(super) fn headless(frame: Frame) -> Self {
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
    pub(super) fn open(trace: &str, reach: &reach::Reach) -> Self {
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

    pub(super) fn mark(&mut self, declaration: &str) {
        self.spans += 1;
        self.declarations.insert(declaration.to_string());
    }
}
