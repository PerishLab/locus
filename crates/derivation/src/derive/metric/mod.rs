mod bucket;
#[cfg(test)]
mod tests;
use bucket::{Bucket, Sample};
use locus::Atom;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const CELLS: usize = 8192;
const BYTES: usize = 16 * 1024 * 1024;
const RECORDS: usize = 65536;

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Gauge,
    Counter,
}

#[cfg_attr(feature = "utoipa", derive(utoipa::IntoParams, utoipa::ToSchema))]
#[cfg_attr(feature = "utoipa", into_params(parameter_in = Query))]
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub producer: String,
    pub name: String,
    pub unit: String,
    pub kind: Kind,
    pub from: u64,
    pub to: u64,
    pub step: u64,
    pub group: Option<String>,
}

impl Query {
    fn slots(&self) -> Result<usize, String> {
        let span = self.to.checked_sub(self.from).ok_or("invalid window")?;
        if span == 0 || self.step == 0 || span > 7 * 86_400_000_000_000 {
            return Err("expected a positive step and a window of at most seven days".into());
        }
        let slots = span.div_ceil(self.step);
        if slots > CELLS as u64 {
            return Err("too many time buckets".into());
        }
        for field in [Some(&self.name), Some(&self.unit), self.group.as_ref()]
            .into_iter()
            .flatten()
        {
            if field.is_empty() || field.len() > 128 {
                return Err("invalid metric descriptor or dimension".into());
            }
        }
        Ok(slots as usize)
    }
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Serialize, Deserialize)]
pub struct Report {
    pub query: Query,
    pub groups: Vec<Group>,
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Serialize, Deserialize)]
pub struct Group {
    pub label: Option<String>,
    pub buckets: Vec<Bucket>,
}

pub struct Metric {
    query: Query,
    slots: usize,
    bytes: usize,
    records: usize,
    events: BTreeMap<String, Sample>,
    labels: BTreeSet<Option<String>>,
}

impl Metric {
    pub fn new(query: Query) -> Result<Self, String> {
        let slots = query.slots()?;
        let mut labels = BTreeSet::new();
        if query.group.is_none() {
            labels.insert(None);
        }
        Ok(Self {
            query,
            slots,
            bytes: 0,
            records: 0,
            events: BTreeMap::new(),
            labels,
        })
    }

    pub fn observe(&mut self, raw: &[u8]) -> Result<(), String> {
        self.bytes += raw.len();
        self.records += 1;
        if self.bytes > BYTES || self.records > RECORDS {
            return Err("metric scan exceeds input budget".into());
        }
        let atom: Atom = serde_json::from_slice(raw).map_err(|_| "invalid stored Atom")?;
        if atom.producer() != Some(&self.query.producer)
            || !(self.query.from..self.query.to).contains(&atom.at())
        {
            return Ok(());
        }
        let context = atom.context();
        if context.get("metric.name") != Some(&self.query.name) {
            return Ok(());
        }
        let sample = Sample::read(&atom, &self.query)?;
        self.labels.insert(sample.label.clone());
        if self.labels.len() * self.slots > CELLS {
            return Err("metric grouping exceeds output budget".into());
        }
        let id = atom
            .id()
            .filter(|v| !v.is_empty())
            .ok_or("metric event needs an id")?;
        if let Some(prior) = self.events.insert(id.into(), sample.clone())
            && prior != sample
        {
            return Err("conflicting metric event identity".into());
        }
        Ok(())
    }

    pub fn finish(self) -> Report {
        let mut groups: BTreeMap<_, _> = self
            .labels
            .into_iter()
            .map(|label| {
                let buckets = (0..self.slots)
                    .map(|i| {
                        let from = self.query.from + i as u64 * self.query.step;
                        Bucket::new(
                            from,
                            from.saturating_add(self.query.step).min(self.query.to),
                        )
                    })
                    .collect::<Vec<_>>();
                (label, buckets)
            })
            .collect();
        let mut series = BTreeMap::<_, BTreeMap<u64, Vec<_>>>::new();
        for (id, sample) in self.events {
            series
                .entry((sample.series.clone(), sample.label.clone()))
                .or_default()
                .entry(sample.at)
                .or_default()
                .push((id, sample));
        }
        for ((_, label), times) in series {
            let mut previous = None;
            let buckets = groups.get_mut(&label).expect("registered label");
            for (at, events) in times {
                let index = ((at - self.query.from) / self.query.step) as usize;
                buckets[index].observe(events, &mut previous, self.query.kind);
            }
        }
        let groups = groups
            .into_iter()
            .map(|(label, mut buckets)| {
                for bucket in &mut buckets {
                    bucket.finish(self.query.kind);
                }
                Group { label, buckets }
            })
            .collect();
        Report {
            query: self.query,
            groups,
        }
    }
}
