use super::model::{
    Attempt, Buffer, Fact, Outcomes, Progress, Readback, Report, Scope, Stage, Window,
};
use super::pairs::Pairs;
use std::collections::{BTreeMap, BTreeSet};

pub struct Quality {
    window: Window,
    records: u64,
    bytes: usize,
    groups: BTreeSet<String>,
    rounds: BTreeSet<(String, String)>,
    ids: BTreeMap<String, Fact>,
    attempts: BTreeMap<String, Attempt>,
    accepted: BTreeSet<String>,
    expected: Option<BTreeSet<String>>,
    events: BTreeMap<String, u64>,
    classes: BTreeMap<String, u64>,
    pairs: Pairs,
    outcomes: Outcomes,
    duplicate: u64,
    unscoped: u64,
}

impl Quality {
    pub fn new(window: Window, expected: Option<BTreeSet<String>>) -> Result<Self, String> {
        if window.from >= window.to {
            return Err("quality requires a nonempty bounded window".into());
        }
        if let Some(keys) = &expected
            && (keys.len() > super::LIMIT || keys.iter().any(|k| !super::key(k)))
        {
            return Err("invalid expected scope".into());
        }
        Ok(Self {
            window,
            records: 0,
            bytes: 0,
            groups: BTreeSet::new(),
            rounds: BTreeSet::new(),
            ids: BTreeMap::new(),
            attempts: BTreeMap::new(),
            accepted: BTreeSet::new(),
            expected,
            events: BTreeMap::new(),
            classes: BTreeMap::new(),
            pairs: Pairs::default(),
            outcomes: Outcomes::default(),
            duplicate: 0,
            unscoped: 0,
        })
    }

    pub fn observe(&mut self, fact: Fact) -> Result<(), String> {
        if !self.window.contains(fact.at) {
            return Ok(());
        }
        self.budget(&fact.id)?;
        self.budget(&fact.group)?;
        self.budget(&fact.event)?;
        self.optional(&fact.round)?;
        self.optional(&fact.call)?;
        self.optional(&fact.class)?;
        self.records += 1;
        if self.records as usize > super::LIMIT {
            return Err("quality record limit exceeded".into());
        }
        if let Some(previous) = self.ids.get(&fact.id) {
            if previous != &fact {
                return Err("conflicting API Atom identity".into());
            }
            self.duplicate += 1;
            return Ok(());
        }
        self.ids.insert(fact.id.clone(), fact.clone());
        self.groups.insert(fact.group.clone());
        if let Some(round) = &fact.round {
            self.rounds.insert((fact.group.clone(), round.clone()));
        }
        *self.events.entry(fact.event.clone()).or_default() += 1;
        if fact.side == Some(super::Side::Before) || fact.event == "CommandShape" {
            *self
                .classes
                .entry(fact.class.clone().unwrap_or("unavailable".into()))
                .or_default() += 1;
        }
        self.pairs.observe(&fact)
    }

    pub fn attempt(&mut self, attempt: Attempt) -> Result<(), String> {
        attempt.validate()?;
        let Some(at) = attempt.at else {
            self.unscoped += 1;
            return Ok(());
        };
        if !self.window.contains(at) {
            return Ok(());
        }
        self.optional(&attempt.id)?;
        self.optional(&attempt.group)?;
        self.optional(&attempt.atom)?;
        self.optional(&attempt.event)?;
        if let Some(id) = &attempt.id {
            if let Some(previous) = self.attempts.get(id) {
                if previous != &attempt {
                    return Err("conflicting attempt identity".into());
                }
                self.outcomes.duplicate += 1;
                return Ok(());
            }
            if self.attempts.len() == super::LIMIT {
                return Err("quality attempt identity limit exceeded".into());
            }
            self.attempts.insert(id.clone(), attempt.clone());
        } else {
            self.outcomes.unkeyed += 1;
        }
        self.count(&attempt)
    }

    fn count(&mut self, attempt: &Attempt) -> Result<(), String> {
        self.outcomes.observed += 1;
        if self.outcomes.observed as usize > super::LIMIT {
            return Err("quality attempt limit exceeded".into());
        }
        self.outcomes.unattributed += u64::from(attempt.group.is_none());
        match &attempt.outcome {
            Progress::Rejected { stage } => {
                self.outcomes.rejected += 1;
                self.outcomes.normalized +=
                    u64::from(matches!(stage, Stage::Bootstrap | Stage::Accept));
            }
            Progress::Accepted { buffer } => {
                self.outcomes.normalized += 1;
                self.outcomes.accepted += 1;
                self.outcomes.unlinked += u64::from(attempt.atom.is_none());
                match buffer {
                    Buffer::Written => self.outcomes.buffered += 1,
                    Buffer::Failed => self.outcomes.failed += 1,
                    Buffer::Unknown => self.outcomes.unknown += 1,
                }
            }
        }
        if let Some(id) = &attempt.atom
            && !self.accepted.insert(id.clone())
        {
            return Err("distinct attempts reuse an accepted Atom identity".into());
        }
        for issue in &attempt.issues {
            self.budget(&issue.code)?;
            self.optional(&issue.field)?;
            *self.outcomes.issues.entry(issue.code.clone()).or_default() += 1;
        }
        Ok(())
    }

    fn optional(&mut self, text: &Option<String>) -> Result<(), String> {
        if let Some(text) = text {
            self.budget(text)?;
        }
        Ok(())
    }

    fn budget(&mut self, text: &str) -> Result<(), String> {
        if !super::key(text) {
            return Err("invalid quality identity".into());
        }
        self.bytes = self
            .bytes
            .checked_add(text.len() * 4)
            .ok_or("quality byte overflow")?;
        if self.bytes > super::BYTES {
            return Err("quality identity byte limit exceeded".into());
        }
        Ok(())
    }

    pub fn finish(self, diagnostics: bool, legacy: u64, retained: Option<u64>) -> Report {
        let ids: BTreeSet<_> = self.ids.into_keys().collect();
        let readback = Readback {
            matched: ids.intersection(&self.accepted).count(),
            pending: self.accepted.difference(&ids).count(),
            unexplained: ids.difference(&self.accepted).count(),
            duplicate: self.duplicate,
        };
        let scope = Scope {
            observed: self.groups.len(),
            expected: self.expected.as_ref().map(BTreeSet::len),
            matched: self
                .expected
                .as_ref()
                .map(|keys| keys.intersection(&self.groups).count()),
            rounds: self.rounds.len(),
            basis: if self.expected.is_some() {
                "caller-declared-scope"
            } else {
                "observed-only"
            },
        };
        let mut unknowns = vec![
            "uninvoked-killed-or-unwritten-attempts",
            "supplied-log-and-client-population-completeness-unproven",
            "business-completion",
            "source-emitter-version",
            "exact-process-duration",
            "unmatched-api-may-reflect-window-retention-or-drain-delay",
        ];
        if self.expected.is_none() {
            unknowns.push("active-population-denominator");
        }
        if !diagnostics {
            unknowns.push("adapter-rejections-and-buffer-results");
        }
        if legacy > 0 {
            unknowns.push("legacy-diagnostic-window-and-attribution");
        }
        if self.outcomes.unlinked > 0 {
            unknowns.push("accepted-atom-identity-unavailable");
        }
        if self.outcomes.unkeyed > 0 {
            unknowns.push("unkeyed-attempt-duplication");
        }
        if self.outcomes.issues.contains_key("reporter.loss") {
            unknowns.push("spool-loss-record-count");
        }
        Report {
            schema: "locus.quality/v1",
            window: self.window,
            records: self.records,
            events: self.events,
            scope,
            tools: self.pairs.finish(),
            classes: self.classes,
            outcomes: diagnostics.then_some(self.outcomes),
            readback: diagnostics.then_some(readback),
            legacy,
            unscoped: self.unscoped,
            retained,
            unknowns,
        }
    }
}
