use crate::doctor::{Check, Status};
use locus_api::expected::Expected;
use locus_api::registry::Registry;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

const NAME: &str = "producers";
const QUIET: Duration = Duration::from_secs(24 * 60 * 60);
const MARKER: &str = "registration";
const ACTIVE: &str = "active.jsonl";
const SEGMENTS: [&str; 2] = ["sealed-", "claimed-"];
const GATE: &str = "A producer stays silent while its own [locus] gate file is missing or says enabled = false; add [locus] enabled = true to that product's config file, run it once, and check again.";
const LIST: &str =
    "Give [producers] expected only distinct names of lowercase letters, digits, '.' and '-'.";

pub fn check(home: &Path, now: SystemTime) -> Check {
    let expected = match Expected::read(home) {
        Ok(expected) => expected,
        Err(error) => return Check::new(NAME, Status::Warn, error, Some(LIST)),
    };
    let seen = latest(home);
    let mut lines: Vec<String> = expected
        .names()
        .filter(|name| !seen.contains_key(name.text()))
        .map(|name| format!("{name} never registered"))
        .collect();
    let mut quiet = !lines.is_empty();
    for (name, last) in &seen {
        let age = last.map(|last| now.duration_since(last).unwrap_or_default());
        let line = match age {
            Some(age) if age <= QUIET => format!("{name} last wrote {} ago", span(age)),
            Some(age) => format!("{name} silent for {}", span(age)),
            None => format!("{name} registered but has written nothing"),
        };
        quiet |= age.is_none_or(|age| age > QUIET);
        lines.push(line);
    }
    if lines.is_empty() {
        return Check::new(
            NAME,
            Status::Ok,
            "no producer is registered or expected".to_string(),
            None,
        );
    }
    let status = if quiet { Status::Warn } else { Status::Ok };
    Check::new(NAME, status, lines.join("; "), quiet.then_some(GATE))
}

fn latest(home: &Path) -> BTreeMap<String, Option<SystemTime>> {
    let mut seen = BTreeMap::new();
    let Ok(registry) = Registry::open(home.join("spools.json")) else {
        return seen;
    };
    for source in registry.sources() {
        let last = written(&source.path);
        let entry = seen
            .entry(source.producer.text().to_string())
            .or_insert(None);
        *entry = (*entry).max(last);
    }
    seen
}

fn written(path: &Path) -> Option<SystemTime> {
    fs::read_dir(path)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| owned(&entry.file_name().to_string_lossy()))
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .max()
}

fn owned(name: &str) -> bool {
    name == MARKER || name == ACTIVE || SEGMENTS.iter().any(|prefix| name.starts_with(prefix))
}

fn span(age: Duration) -> String {
    let seconds = age.as_secs();
    match seconds {
        0..120 => format!("{seconds}s"),
        120..7200 => format!("{}m", seconds / 60),
        _ => format!("{}h", seconds / 3600),
    }
}
