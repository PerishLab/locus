use super::config::Analyzer;
use super::mapping::Observation;
use locus::Role;
use serde::{Deserialize, Serialize};

const SCHEMA: &str = "locus.inspect/v1";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Threshold {
    pub above: u64,
}

impl Threshold {
    pub fn exceeded(&self, measurement: &Measurement) -> bool {
        match measurement {
            Measurement::Bytes { value } => *value > self.above,
            Measurement::Prefix {
                atoms, repeated, ..
            } => u128::from(*repeated) * 100 > u128::from(*atoms) * u128::from(self.above),
        }
    }
}

#[derive(Serialize)]
pub struct Group {
    role: String,
    key: String,
}

impl Group {
    pub fn new(role: &Role, key: String) -> Self {
        Self {
            role: role.text().into(),
            key,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "mapping")]
pub enum Measurement {
    #[serde(rename = "representation-bytes")]
    Bytes { value: u64 },
    #[serde(rename = "dominant-content-prefix-percent")]
    Prefix {
        value: u64,
        atoms: u64,
        repeated: u64,
        bytes: usize,
    },
}

#[derive(Serialize)]
pub struct Finding {
    schema: &'static str,
    kind: &'static str,
    analyzer: String,
    group: Group,
    measurement: Measurement,
    threshold: Threshold,
}

impl Finding {
    pub fn new(analyzer: &Analyzer, observation: Observation) -> Self {
        Self {
            schema: SCHEMA,
            kind: "finding",
            analyzer: analyzer.id.clone(),
            group: observation.group,
            measurement: observation.measurement,
            threshold: analyzer.threshold.clone(),
        }
    }

    pub fn identity(&self) -> (&str, &str, &str) {
        (&self.analyzer, &self.group.role, &self.group.key)
    }
}

#[derive(Serialize)]
pub struct Summary {
    schema: &'static str,
    kind: &'static str,
    coverage: Vec<String>,
    records: u64,
    findings: usize,
}

impl Summary {
    pub fn new(coverage: Vec<String>, records: u64, findings: usize) -> Self {
        Self {
            schema: SCHEMA,
            kind: "summary",
            coverage,
            records,
            findings,
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Record<'a> {
    Finding(&'a Finding),
    Summary(&'a Summary),
}
