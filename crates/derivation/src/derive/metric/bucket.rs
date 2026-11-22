use super::{Kind, Query};
use locus::Atom;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq)]
pub(super) struct Sample {
    pub at: u64,
    pub series: String,
    pub label: Option<String>,
    value: f64,
    epoch: Option<String>,
}

impl Sample {
    pub fn read(atom: &Atom, query: &Query) -> Result<Self, String> {
        let context = atom.context();
        let kind = match query.kind {
            Kind::Counter => "counter",
            Kind::Gauge => "gauge",
        };
        if context.get("metric.kind").map(String::as_str) != Some(kind)
            || context.get("metric.unit") != Some(&query.unit)
        {
            return Err("metric descriptor differs from stored samples".into());
        }
        let series = context
            .get("metric.series")
            .filter(|v| !v.is_empty())
            .ok_or("metric needs a series")?
            .clone();
        let label = query
            .group
            .as_ref()
            .map(|key| {
                context
                    .get(&format!("metric.label.{key}"))
                    .cloned()
                    .ok_or("undeclared metric dimension")
            })
            .transpose()?;
        let value = atom
            .payload()
            .and_then(|p| p.get("value"))
            .and_then(|v| v.as_f64())
            .ok_or("metric value must be finite")?;
        if query.kind == Kind::Counter && value < 0.0 {
            return Err("counter must be nonnegative".into());
        }
        Ok(Self {
            at: atom.at(),
            series,
            label,
            value,
            epoch: context
                .get("metric.epoch")
                .filter(|v| !v.is_empty())
                .cloned(),
        })
    }
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Serialize, Deserialize)]
pub struct Evidence {
    pub series: String,
    pub at: u64,
    pub id: String,
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Serialize, Deserialize)]
pub struct Bucket {
    pub from: u64,
    pub to: u64,
    pub samples: usize,
    pub quality: String,
    pub value: Option<f64>,
    pub evidence: Vec<Evidence>,
    #[serde(skip)]
    terms: usize,
}

impl Bucket {
    pub(super) fn new(from: u64, to: u64) -> Self {
        Self {
            from,
            to,
            samples: 0,
            quality: "missing".into(),
            value: None,
            evidence: Vec::new(),
            terms: 0,
        }
    }

    pub(super) fn observe(
        &mut self,
        events: Vec<(String, Sample)>,
        previous: &mut Option<(Sample, Evidence)>,
        kind: Kind,
    ) {
        let sample = events[0].1.clone();
        let first = Evidence {
            series: sample.series.clone(),
            at: sample.at,
            id: events[0].0.clone(),
        };
        let conflict = events.iter().any(|(_, s)| s != &sample);
        self.evidence
            .extend(events.into_iter().map(|(id, s)| Evidence {
                id,
                at: s.at,
                series: s.series,
            }));
        self.samples += 1;
        if conflict {
            self.quality = "conflict".into();
            *previous = None;
            return;
        }
        let term = match (kind, previous.take()) {
            (Kind::Gauge, _) => Some(sample.value),
            (Kind::Counter, None) => None,
            (Kind::Counter, Some((last, evidence))) => {
                if last.at < self.from {
                    self.evidence.push(evidence);
                }
                if last.epoch.is_some() && sample.epoch.is_some() && last.epoch != sample.epoch {
                    Some(sample.value)
                } else if sample.value >= last.value && last.epoch == sample.epoch {
                    Some(sample.value - last.value)
                } else {
                    self.discontinuity();
                    None
                }
            }
        };
        *previous = Some((sample, first));
        if let Some(value) = term {
            self.terms += 1;
            *self.value.get_or_insert(0.0) += value;
        }
    }

    fn discontinuity(&mut self) {
        if self.quality != "conflict" {
            self.quality = "discontinuous".into();
        }
    }

    pub(super) fn finish(&mut self, kind: Kind) {
        if self.quality == "conflict" || self.quality == "discontinuous" {
            self.value = None;
        } else if let Some(value) = &mut self.value {
            if kind == Kind::Gauge {
                *value /= self.terms as f64;
            }
            if value.is_finite() {
                self.quality = "observed".into();
            } else {
                self.value = None;
                self.quality = "overflow".into();
            }
        } else if self.samples > 0 {
            self.quality = "baseline".into();
        }
    }
}
