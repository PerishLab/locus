use crate::day;
use crate::store::Store;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Default)]
pub struct Retention {
    days: Option<u64>,
}

impl Retention {
    pub fn days(days: u64) -> Self {
        Self { days: Some(days) }
    }

    pub fn declared(&self) -> Option<u64> {
        self.days
    }

    pub fn read(home: &Path) -> Result<Self, String> {
        let declared = crate::config::read(home)?;
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
