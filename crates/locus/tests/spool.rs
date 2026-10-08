use locus::reporter::{self, spool};
use locus::{Candidate, Config, Context, Engine, Hook, Observation, Policy};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const CHILD: &str = "LOCUS_SPOOL_CHILD";
const WORKER: &str = "LOCUS_SPOOL_WORKER";

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

#[test]
fn writer() {
    let (Ok(directory), Ok(worker)) = (std::env::var(CHILD), std::env::var(WORKER)) else {
        return;
    };
    let engine = engine(Path::new(&directory), 1 << 30, 4_096, None);
    for sequence in 0..300 {
        emit(&engine, &worker, sequence);
    }
}

#[test]
fn processes() {
    let directory = temp("processes");
    let children: Vec<_> = (0..6)
        .map(|worker| {
            Command::new(std::env::current_exe().expect("test binary"))
                .args(["--exact", "writer", "--quiet"])
                .env(CHILD, &directory)
                .env(WORKER, worker.to_string())
                .spawn()
                .expect("child")
        })
        .collect();
    for mut child in children {
        assert!(child.wait().expect("wait").success());
    }
    let atoms = Spooled(&directory).atoms();
    assert_eq!(atoms.len(), 1_800);
    assert_eq!(distinct(&atoms, "id"), 1_800);
    let payloads: BTreeSet<_> = atoms
        .iter()
        .map(|atom| atom["payload"].to_string())
        .collect();
    assert_eq!(payloads.len(), 1_800);
    let sealed = spool::segments(&directory).expect("segments");
    assert!(sealed.len() > 10);
    for path in sealed {
        let size = fs::metadata(&path).expect("segment").len();
        assert!((4_096..4_096 + 1_024).contains(&size), "{size}");
    }
}

#[test]
fn ceiling() {
    let directory = temp("ceiling");
    let capture = Arc::new(Capture::default());
    let engine = engine(&directory, 8_192, 2_048, Some(capture.clone()));
    for sequence in 0..300 {
        emit(&engine, "single", sequence);
        assert!(Spooled(&directory).footprint() <= 8_192);
    }
    let lost: u64 = Spooled(&directory)
        .ledger()
        .iter()
        .map(|entry| entry["records"].as_u64().expect("records"))
        .sum();
    assert!(lost > 0);
    assert_eq!(lost + Spooled(&directory).atoms().len() as u64, 300);
    let codes = capture.codes.lock().expect("capture");
    assert!(!codes.is_empty());
    assert!(codes.iter().all(|code| code == "reporter.loss"));
}

#[test]
fn consumer() {
    let directory = temp("consumer");
    let engine = engine(&directory, 8_192, 2_048, None);
    let mut sequence = 0;
    while spool::segments(&directory).expect("segments").len() < 2 {
        emit(&engine, "single", sequence);
        sequence += 1;
    }
    let claimed = spool::claim(&directory).expect("claim");
    assert_eq!(claimed.len(), 2);
    assert!(spool::segments(&directory).expect("segments").is_empty());
    let held: Vec<_> = claimed
        .iter()
        .map(|path| fs::read(path).expect("claimed"))
        .collect();
    for _ in 0..200 {
        emit(&engine, "single", sequence);
        sequence += 1;
    }
    for (path, bytes) in claimed.iter().zip(held) {
        assert_eq!(fs::read(path).expect("claimed"), bytes);
    }
    let lost: u64 = Spooled(&directory)
        .ledger()
        .iter()
        .map(|entry| entry["records"].as_u64().expect("records"))
        .sum();
    let claimed: usize = claimed
        .iter()
        .map(|path| fs::read_to_string(path).expect("claimed").lines().count())
        .sum();
    assert_eq!(
        lost as usize + claimed + Spooled(&directory).atoms().len(),
        sequence
    );
}

#[test]
fn sealed() {
    let directory = temp("sealed");
    let engine = engine(&directory, 1 << 30, 1_024, None);
    for sequence in 0..40 {
        emit(&engine, "single", sequence);
    }
    let before: Vec<_> = spool::segments(&directory)
        .expect("segments")
        .into_iter()
        .map(|path| (fs::read(&path).expect("segment"), path))
        .collect();
    for sequence in 40..80 {
        emit(&engine, "single", sequence);
    }
    let mut first = 0;
    for (bytes, path) in before {
        assert_eq!(fs::read(&path).expect("segment"), bytes);
        let line = bytes.split(|byte| *byte == b'\n').next().expect("line");
        let at = serde_json::from_slice::<Value>(line).expect("atom")["at"]
            .as_u64()
            .expect("at");
        assert!(at > first);
        first = at;
    }
}

#[test]
fn identity() {
    let directory = temp("identity");
    let engine = engine(&directory, 1 << 30, 1_024, None);
    for sequence in 0..50 {
        emit(&engine, "single", sequence);
    }
    let atoms = Spooled(&directory).atoms();
    assert!(atoms.iter().all(|atom| atom["producer"] == "concord"));
    let twice: Vec<Value> = atoms.iter().chain(atoms.iter()).cloned().collect();
    assert_eq!(twice.len(), 100);
    assert_eq!(distinct(&twice, "id"), 50);
}

#[test]
fn refused() {
    let directory = temp("refused");
    let declarations = [
        Policy::default()
            .producer("Concord")
            .reporter(reporter::Spec::file(directory.join("a"))),
        Policy::default().reporter(reporter::Spec::new(
            "spool",
            json!({"path": directory, "ceiling": 10, "segment": 1, "compression": "zstd"}),
        )),
        Policy::default().reporter(reporter::Spec::new("future", json!({}))),
    ];
    for policy in declarations {
        let capture = Arc::new(Capture::default());
        let result = Engine::bootstrap(Config::new(policy).hook(capture.clone()));
        assert!(result.is_err());
        assert_eq!(
            *capture.codes.lock().expect("capture"),
            vec!["bootstrap.failed"]
        );
    }
}

fn engine(directory: &Path, ceiling: u64, segment: u64, hook: Option<Arc<Capture>>) -> Engine {
    let policy = Policy::default()
        .producer("concord")
        .reporter(reporter::Spec::spool(directory, ceiling, segment));
    let config = match hook {
        Some(hook) => Config::new(policy).hook(hook),
        None => Config::new(policy),
    };
    Engine::bootstrap(config).expect("engine")
}

fn emit(engine: &Engine, worker: &str, sequence: usize) {
    engine
        .append(
            &Context::empty(),
            Candidate::event(json!({"worker": worker, "sequence": sequence})),
        )
        .expect("append");
}

struct Spooled<'a>(&'a Path);

impl Spooled<'_> {
    fn atoms(&self) -> Vec<Value> {
        let directory = self.0;
        let mut paths = spool::segments(directory).expect("segments");
        paths.push(directory.join("active.jsonl"));
        paths
            .iter()
            .filter_map(|path| fs::read_to_string(path).ok())
            .flat_map(|text| {
                text.lines()
                    .map(|line| serde_json::from_str(line).expect("whole record"))
                    .collect::<Vec<Value>>()
            })
            .collect()
    }

    fn ledger(&self) -> Vec<Value> {
        let directory = self.0;
        fs::read_to_string(directory.join("loss.jsonl"))
            .map(|text| {
                text.lines()
                    .map(|line| serde_json::from_str(line).expect("entry"))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn footprint(&self) -> u64 {
        let directory = self.0;
        let mut paths = spool::segments(directory).expect("segments");
        paths.push(directory.join("active.jsonl"));
        paths
            .iter()
            .filter_map(|path| fs::metadata(path).ok())
            .map(|metadata| metadata.len())
            .sum()
    }
}

fn distinct(atoms: &[Value], field: &str) -> usize {
    atoms
        .iter()
        .map(|atom| atom[field].to_string())
        .collect::<BTreeSet<_>>()
        .len()
}

fn temp(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "locus-spool-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("temp");
    path
}
