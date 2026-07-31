mod config;
mod mapping;
mod model;

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

    fn findings(self) -> Vec<Finding> {
        self.state
            .finish()
            .into_iter()
            .filter(|observation| self.analyzer.threshold.exceeded(&observation.measurement))
            .map(|observation| Finding::new(self.analyzer, observation))
            .collect()
    }
}

pub fn scan(mut reader: impl BufRead, config: &Config) -> Result<Report, String> {
    let mut runtimes = config
        .analyzers()
        .iter()
        .map(Runtime::new)
        .collect::<Vec<_>>();
    let mut offset = 0_u64;
    loop {
        let mut line = Vec::new();
        let bytes = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("cannot read line {}: {error}", offset + 1))?;
        if bytes == 0 {
            break;
        }
        offset = offset
            .checked_add(1)
            .ok_or_else(|| "Atom count overflowed".to_string())?;
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.is_empty() {
            return Err(format!("invalid Atom on line {offset}: empty record"));
        }
        let atom = serde_json::from_slice(&line)
            .map_err(|error| format!("invalid Atom on line {offset}: {error}"))?;
        let encoded = u64::try_from(bytes)
            .map_err(|_| format!("encoded size overflowed on line {offset}"))?;
        for runtime in &mut runtimes {
            runtime.state.observe(&atom, encoded)?;
        }
    }
    let mut findings = runtimes
        .into_iter()
        .flat_map(Runtime::findings)
        .collect::<Vec<_>>();
    findings.sort_by(|left, right| left.identity().cmp(&right.identity()));
    let summary = Summary::new(config.coverage(), offset, findings.len());
    Ok(Report { findings, summary })
}
