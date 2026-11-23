use super::rule::{Rule, SECOND};
use locus_derivation::metric::Report;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    Normal,
    Pending,
    Firing,
    Unknown,
    Recovered,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct State {
    pub revision: u64,
    pub fingerprint: String,
    pub evaluated: u64,
    pub latest: Option<u64>,
    pub proof: Option<Proof>,
    pub phase: Phase,
    pub boundary: Option<u64>,
    pub since: Option<u64>,
    pub reason: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Proof {
    pub from: u64,
    pub to: u64,
    pub latest: Option<u64>,
    pub quality: String,
    pub samples: usize,
    pub value: Option<f64>,
}

impl Proof {
    pub fn report(report: &Report) -> Self {
        let query = &report.query;
        let bucket = &report.groups[0].buckets[0];
        Self {
            from: query.from,
            to: query.to,
            latest: bucket.evidence.iter().map(|record| record.at).max(),
            quality: bucket.quality.clone(),
            samples: bucket.samples,
            value: bucket.value,
        }
    }

    pub fn missing(from: u64, to: u64, reason: &str) -> Self {
        Self {
            from,
            to,
            latest: None,
            quality: reason.into(),
            samples: 0,
            value: None,
        }
    }
}

impl State {
    pub fn new(rule: &Rule, interval: u64, delay: u64) -> Self {
        Self {
            revision: rule.revision,
            fingerprint: rule.fingerprint(interval, delay),
            evaluated: 0,
            latest: None,
            proof: None,
            phase: Phase::Unknown,
            boundary: None,
            since: None,
            reason: Some("first_evaluation".into()),
        }
    }

    pub fn advance(&mut self, rule: &Rule, proof: &Proof) -> Option<Phase> {
        let was = self.phase;
        self.latest = self.latest.max(proof.latest);
        if let (Some(value), Some(at)) = (proof.value, proof.latest)
            && proof.quality == "observed"
        {
            self.seen(rule, value, at);
        } else {
            self.unknown(proof.to.saturating_sub(1), &proof.quality);
        }
        self.evaluated = proof.to;
        self.proof = Some(proof.clone());
        if self.phase == was {
            return None;
        }
        match self.phase {
            Phase::Firing | Phase::Recovered => Some(self.phase),
            Phase::Unknown if self.boundary.is_some() => Some(Phase::Unknown),
            _ => None,
        }
    }

    pub fn stopped(&mut self, bound: u64) {
        self.unknown(bound, "stopped");
    }

    fn unknown(&mut self, bound: u64, reason: &str) {
        if self.evaluated != 0 {
            self.boundary = self.boundary.max(Some(bound));
        }
        self.phase = Phase::Unknown;
        self.since = None;
        self.reason = Some(reason.into());
    }

    fn seen(&mut self, rule: &Rule, value: f64, at: u64) {
        if value > rule.above {
            let since = *self.since.get_or_insert(at);
            self.phase = if at.saturating_sub(since) >= rule.hold * SECOND {
                self.boundary = Some(at);
                Phase::Firing
            } else {
                Phase::Pending
            };
            self.reason = None;
            return;
        }
        if self.boundary.is_some_and(|bound| at <= bound) {
            return;
        }
        self.phase = if self.boundary.take().is_some() {
            Phase::Recovered
        } else {
            Phase::Normal
        };
        self.since = None;
        self.reason = None;
    }
}
