mod config;
mod mapping;
mod model;

use crate::input;
use config::Analyzer;
pub use config::Config;
use mapping::State;
use model::{Finding, Record, Summary};
use std::io::BufRead;

pub struct Report {
    findings: Vec<Finding>,
    summary: Summary,
}

impl Report {
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

    fn findings(self) -> (Vec<Finding>, u64) {
        let reading = self.state.finish();
        let findings = reading
            .observations
            .into_iter()
            .filter(|observation| self.analyzer.threshold.exceeded(&observation.measurement))
            .map(|observation| Finding::new(self.analyzer, observation))
            .collect();
        (findings, reading.refused)
    }
}

pub fn scan(reader: impl BufRead, config: &Config) -> Result<Report, String> {
    let mut runtimes = config
        .analyzers()
        .iter()
        .map(Runtime::new)
        .collect::<Vec<_>>();
    let records = input::scan(reader, |atom, encoded| {
        for runtime in &mut runtimes {
            runtime.state.observe(&atom, encoded)?;
        }
        Ok(())
    })?;
    let mut findings = Vec::new();
    let mut refused = 0_u64;
    for runtime in runtimes {
        let (found, dropped) = runtime.findings();
        findings.extend(found);
        refused += dropped;
    }
    findings.sort_by(|left, right| left.identity().cmp(&right.identity()));
    let summary = Summary::new(config.coverage(), records, findings.len(), refused);
    Ok(Report { findings, summary })
}
