use locus::generator;
use locus::reporter;
use locus::{Candidate, Config, Context, Engine, Policy, Role};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn concurrency() {
    const WORKERS: usize = 64;

    let home = temp();
    fs::create_dir(&home).expect("temp");
    let shared = home.join("trace.json");
    let report = home.join("atoms.jsonl");
    let barrier = Arc::new(Barrier::new(WORKERS));
    let threads: Vec<_> = (0..WORKERS)
        .map(|worker| {
            let shared = shared.clone();
            let report = report.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let policy = Policy::default()
                    .generator(Role::trace(), generator::Spec::shared(shared))
                    .reporter(reporter::Spec::file(report));
                let engine = Engine::bootstrap(Config::new(policy)).expect("engine");
                barrier.wait();
                engine
                    .append(
                        &Context::empty(),
                        Candidate::event(json!({
                            "event": "concurrency",
                            "worker": worker,
                            "padding": "x".repeat(4096),
                        }))
                        .ensure(Role::trace()),
                    )
                    .expect("append");
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("worker");
    }

    let text = fs::read_to_string(&report).expect("report");
    let atoms: Vec<locus::Atom> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("complete Atom"))
        .collect();
    assert_eq!(atoms.len(), WORKERS);
    let trace = atoms[0].context()["locus.trace"].clone();
    assert!(
        atoms
            .iter()
            .all(|atom| atom.context()["locus.trace"] == trace)
    );
    assert_eq!(fs::read_dir(&home).expect("directory").count(), 2);
    fs::remove_dir_all(home).expect("cleanup");
}

fn temp() -> PathBuf {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("locus-concurrency-{}-{at}", std::process::id()))
}
