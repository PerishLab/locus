use crate::observation::{Watch, note};
use locus::{Context, Engine};
use locus_api::drain::Drain;
use locus_api::retention::{self, Retention};
use locus_api::store::Store;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

type View<'a> = Option<(&'a Engine, &'a Context)>;

pub(crate) struct Cycle {
    pub(crate) drain: Drain,
    pub(crate) store: Arc<dyn Store>,
    pub(crate) retention: Retention,
    pub(crate) observer: Option<Watch>,
}

impl Cycle {
    pub(crate) fn turn(&self) {
        let view = self.observer.as_ref().map(Watch::view);
        drain(view, &self.drain);
        let now = retention::now();
        seal(view, self.store.as_ref(), now);
        retain(view, self, now);
    }
}

#[locus::trace(with = observer.map(Watch::view), frame = frame)]
pub(crate) fn migrate(
    observer: Option<&Watch>,
    legacy: &Path,
    store: &dyn Store,
) -> Result<usize, String> {
    let migrated = locus_api::migrate::migrate(legacy, store);
    let fact = match &migrated {
        Ok(days) => {
            eprintln!("locus-api: migrated {days} stored day(s) into the column store");
            json!({"event": "api.migrate", "outcome": "ok", "days": days})
        }
        Err(_) => json!({"event": "api.migrate", "outcome": "error"}),
    };
    note(observer.map(Watch::view), frame.as_ref(), fact);
    migrated
}

#[locus::trace(with = view, frame = frame)]
fn drain(view: View<'_>, drain: &Drain) {
    let fact = match drain.cycle() {
        Ok(tally) => json!({
            "event": "api.drain",
            "outcome": "ok",
            "sources": tally.sources,
            "segments": tally.segments,
            "records": tally.records,
            "rejected": tally.rejected,
        }),
        Err(error) => failed("api.drain", "drain", &error),
    };
    note(view, frame.as_ref(), fact);
}

#[locus::trace(with = view, frame = frame)]
fn seal(view: View<'_>, store: &dyn Store, now: u64) {
    let fact = match store.tick(now) {
        Ok(()) => json!({"event": "api.seal", "outcome": "ok"}),
        Err(error) => failed("api.seal", "seal", &error),
    };
    note(view, frame.as_ref(), fact);
}

#[locus::trace(with = view, frame = frame)]
fn retain(view: View<'_>, work: &Cycle, now: u64) {
    let mut fact = match work.retention.apply(work.store.as_ref(), now) {
        Ok(()) => json!({"event": "api.retention", "outcome": "ok"}),
        Err(error) => failed("api.retention", "retention", &error),
    };
    fact["days"] = json!(work.retention.declared());
    fact["floor"] = json!(work.retention.floor(now));
    note(view, frame.as_ref(), fact);
}

fn failed(event: &str, step: &str, error: &str) -> Value {
    eprintln!("locus-api: {step}: {error}");
    json!({"event": event, "outcome": "error"})
}
