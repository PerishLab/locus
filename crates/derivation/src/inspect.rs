mod config;
mod mapping;
mod model;

use config::Analyzer;
pub use config::Config;
use locus::Atom;
use mapping::{State, Unmeasured};
use model::{Finding, Record, Summary};

pub struct Report {
    findings: Vec<Finding>,
    summary: Summary,
}

impl Report {
    pub fn retained(mut self, retained: Option<u64>) -> Self {
        self.summary.retain(retained);
        self
    }

    pub fn clean(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn records(&self) -> impl Iterator<Item = Record<'_>> {
        self.findings
            .iter()
            .map(Record::Finding)
            .chain(std::iter::once(Record::Summary(&self.summary)))
    }
}

struct Runtime<'a> {
    analyzer: &'a Analyzer,
    state: State,
}

impl<'a> Runtime<'a> {
    fn new(analyzer: &'a Analyzer) -> Self {
        Self {
            state: State::new(&analyzer.mapping),
            analyzer,
        }
    }

    fn findings(self) -> (Vec<Finding>, Unmeasured) {
        let reading = self.state.finish();
        let findings = reading
            .observations
            .into_iter()
            .filter(|observation| self.analyzer.threshold.exceeded(&observation.measurement))
            .map(|observation| Finding::new(self.analyzer, observation))
            .collect();
        (findings, reading.unmeasured)
    }
}

pub struct Inspection<'a> {
    config: &'a Config,
    runtimes: Vec<Runtime<'a>>,
    records: u64,
}

impl<'a> Inspection<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self {
            config,
            runtimes: config.analyzers().iter().map(Runtime::new).collect(),
            records: 0,
        }
    }

    pub fn observe(&mut self, atom: &Atom, encoded: u64) -> Result<(), String> {
        self.records = self
            .records
            .checked_add(1)
            .ok_or_else(|| "Atom count overflowed".to_string())?;
        for runtime in &mut self.runtimes {
            runtime.state.observe(atom, encoded)?;
        }
        Ok(())
    }

    pub fn finish(self) -> Report {
        let mut findings = Vec::new();
        let mut unmeasured = Unmeasured::default();
        for runtime in self.runtimes {
            let (found, dropped) = runtime.findings();
            findings.extend(found);
            unmeasured.absorb(dropped);
        }
        findings.sort_by(|left, right| left.identity().cmp(&right.identity()));
        let summary = Summary::new(
            self.config.coverage(),
            self.records,
            findings.len(),
            unmeasured,
        );
        Report { findings, summary }
    }
}
