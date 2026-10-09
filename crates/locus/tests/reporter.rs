use locus::reporter;
use locus::{Candidate, Config, Context, Engine, Hook, Observation, Policy};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const CLOSED: &str = "http://127.0.0.1:9";

#[derive(Default)]
struct Capture {
    codes: Mutex<Vec<String>>,
}

impl Hook for Capture {
    fn observe(&self, observation: &Observation) {
        if let Observation::Diagnostic(diagnostic) = observation {
            self.codes
                .lock()
                .expect("capture")
                .push(diagnostic.code().to_string());
        }
    }
}

#[cfg(unix)]
#[test]
fn privacy() {
    use std::os::unix::fs::PermissionsExt;

    let spool = temp("privacy").join("spool");
    emit(&engine(&spool), 0);
    let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode(&spool), 0o700);
    assert_eq!(mode(&spool.join("active.jsonl")), 0o600);
}

#[test]
fn identity() {
    let spool = temp("identity").join("spool");
    let engine = engine(&spool);
    for sequence in 0..50 {
        emit(&engine, sequence);
    }
    let atoms: Vec<Value> = fs::read_to_string(spool.join("active.jsonl"))
        .expect("active")
        .lines()
        .map(|line| serde_json::from_str(line).expect("whole record"))
        .collect();
    assert_eq!(atoms.len(), 50);
    assert!(atoms.iter().all(|atom| atom["producer"] == "concord"));
    let ids: BTreeSet<_> = atoms.iter().map(|atom| atom["id"].to_string()).collect();
    assert_eq!(ids.len(), 50);
}

#[test]
fn refused() {
    let directory = temp("refused");
    let declarations = [
        reporter::Spec::new("file", json!({"path": directory.join("a")})),
        reporter::Spec::new(
            "spool",
            json!({"path": directory, "ceiling": 10, "segment": 1}),
        ),
        reporter::Spec::new(
            "api",
            json!({"endpoint": CLOSED, "path": directory, "ceiling": 10}),
        ),
        reporter::Spec::new("future", json!({})),
    ];
    for spec in declarations {
        let capture = Arc::new(Capture::default());
        let policy = Policy::default().producer("concord").reporter(spec);
        let result = Engine::bootstrap(Config::new(policy).hook(capture.clone()));
        assert!(result.is_err());
        assert_eq!(
            *capture.codes.lock().expect("capture"),
            vec!["bootstrap.failed"]
        );
    }
}

fn engine(spool: &Path) -> Engine {
    let policy = Policy::default()
        .producer("concord")
        .reporter(reporter::Spec::api(CLOSED, spool));
    Engine::bootstrap(Config::new(policy)).expect("engine")
}

fn emit(engine: &Engine, sequence: usize) {
    engine
        .append(
            &Context::empty(),
            Candidate::event(json!({"sequence": sequence})),
        )
        .expect("append");
}

fn temp(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "locus-reporter-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("temp");
    path
}
