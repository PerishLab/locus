use crate::day;
use crate::store::Store;
use serde::Deserialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const FILE: &str = "locus-api.toml";

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Declared {
    retention: Option<Section>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Section {
    days: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Retention {
    days: Option<u64>,
}

impl Retention {
    pub fn days(days: u64) -> Self {
        Self { days: Some(days) }
    }

    pub fn read(home: &Path) -> Result<Self, String> {
        let path = home.join(FILE);
        if !path.is_file() {
            return Ok(Self::default());
        }
        let declared: Declared = plumb::config::load(&path).map_err(|error| error.to_string())?;
        Ok(Self {
            days: declared.retention.map(|section| section.days),
        })
    }

    pub fn floor(&self, now: u64) -> Option<u64> {
        self.days
            .map(|days| (now / day::NANOS).saturating_sub(days) * day::NANOS)
    }

    pub fn apply(&self, store: &dyn Store, now: u64) -> Result<(), String> {
        match self.floor(now) {
            Some(floor) => store.retain(floor / day::NANOS, now),
            None => Ok(()),
        }
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}
