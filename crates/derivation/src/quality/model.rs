use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Input,
    Normalize,
    Bootstrap,
    Accept,
    Buffer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Issue {
    pub stage: Stage,
    pub code: String,
    pub field: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Buffer {
    Written,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Progress {
    Rejected { stage: Stage },
    Accepted { buffer: Buffer },
}

impl Default for Progress {
    fn default() -> Self {
        Self::Rejected {
            stage: Stage::Input,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub schema: String,
    pub at: Option<u64>,
    pub id: Option<String>,
    pub event: Option<String>,
    pub group: Option<String>,
    pub atom: Option<String>,
    pub outcome: Progress,
    pub issues: Vec<Issue>,
}

impl Attempt {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != "locus.adapter.attempt/v1" {
            return Err("unsupported attempt schema".into());
        }
        for value in [&self.id, &self.event, &self.group, &self.atom]
            .into_iter()
            .flatten()
        {
            if !super::key(value) {
                return Err("invalid attempt identity".into());
            }
        }
        if let Progress::Rejected { stage } = &self.outcome {
            self.rejection(stage)?;
        }
        if self.issues.len() > 8 {
            return Err("attempt issue bound exceeded".into());
        }
        for issue in &self.issues {
            validate(issue)?;
        }
        Ok(())
    }

    fn rejection(&self, stage: &Stage) -> Result<(), String> {
        if self.atom.is_some() || *stage == Stage::Buffer {
            return Err(
                "rejected attempt cannot carry an accepted Atom or buffer-stage rejection".into(),
            );
        }
        if !self.issues.iter().any(|issue| &issue.stage == stage) {
            return Err("rejected attempt lacks its stage-specific cause".into());
        }
        Ok(())
    }
}

fn validate(issue: &Issue) -> Result<(), String> {
    for text in std::iter::once(issue.code.as_str()).chain(issue.field.as_deref()) {
        if text.is_empty()
            || text.len() > 64
            || !text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err("invalid bounded issue token".into());
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Side {
    Before,
    After,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fact {
    pub at: u64,
    pub id: String,
    pub group: String,
    pub round: Option<String>,
    pub call: Option<String>,
    pub side: Option<Side>,
    pub class: Option<String>,
    pub event: String,
}

#[derive(Clone, Copy, Serialize)]
pub struct Window {
    pub from: u64,
    pub to: u64,
}

impl Window {
    pub fn contains(&self, at: u64) -> bool {
        at >= self.from && at < self.to
    }
}

#[derive(Default, Serialize)]
pub struct Scope {
    pub observed: usize,
    pub expected: Option<usize>,
    #[serde(rename = "matched_expected")]
    pub matched: Option<usize>,
    pub rounds: usize,
    pub basis: &'static str,
}

#[derive(Default, Serialize)]
pub struct Pairing {
    pub before: u64,
    pub after: u64,
    pub matched: usize,
    #[serde(rename = "before_only")]
    pub opening: usize,
    #[serde(rename = "after_only")]
    pub closing: usize,
    pub ambiguous: usize,
    #[serde(rename = "duplicate_before")]
    pub starts: u64,
    #[serde(rename = "duplicate_after")]
    pub stops: u64,
}

#[derive(Default, Serialize)]
pub struct Outcomes {
    pub observed: u64,
    pub normalized: u64,
    pub accepted: u64,
    pub buffered: u64,
    #[serde(rename = "buffer_failed")]
    pub failed: u64,
    #[serde(rename = "buffer_unknown")]
    pub unknown: u64,
    pub rejected: u64,
    pub duplicate: u64,
    pub unkeyed: u64,
    pub unattributed: u64,
    #[serde(rename = "accepted_without_atom_id")]
    pub unlinked: u64,
    pub issues: BTreeMap<String, u64>,
}

#[derive(Default, Serialize)]
pub struct Readback {
    pub matched: usize,
    #[serde(rename = "accepted_without_api_match")]
    pub pending: usize,
    #[serde(rename = "api_without_outcome_match")]
    pub unexplained: usize,
    #[serde(rename = "api_duplicate_ids")]
    pub duplicate: u64,
}

#[derive(Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub window: Window,
    pub records: u64,
    pub events: BTreeMap<String, u64>,
    pub scope: Scope,
    pub tools: Pairing,
    pub classes: BTreeMap<String, u64>,
    pub outcomes: Option<Outcomes>,
    pub readback: Option<Readback>,
    #[serde(rename = "legacy_unscoped")]
    pub legacy: u64,
    #[serde(rename = "unscoped_attempts")]
    pub unscoped: u64,
    pub retained: Option<u64>,
    pub unknowns: Vec<&'static str>,
}
