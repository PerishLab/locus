use locus::reporter;
use locus::{Candidate, Config, Context, Engine, Policy};
use locus_api::store::{Filter, Segments, Store};
use locus_api::takeover::{Source, Takeover};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const HOUR: Duration = Duration::from_secs(3_600);

#[test]
fn held() {
    let home = temp("held");
    let report = home.join("report.jsonl");
    let (store, takeover) = setup(&home, HOUR);
    let early = engine(&report);
    emit(&early, "early", 0..10);
    takeover.cycle().expect("cycle");
    emit(&early, "early", 10..20);
    let late = engine(&report);
    emit(&late, "late", 0..10);
    takeover.cycle().expect("cycle");
    takeover.cycle().expect("cycle");

    let stored = stored(store.as_ref());
    assert_eq!(stored.len(), 30);
    assert_eq!(unique(&stored), 30);
    assert_eq!(taken(&home).len(), 2);
}

#[test]
fn concurrent() {
    const WORKERS: usize = 8;
    const EVENTS: usize = 100;

    let home = temp("concurrent");
    let report = home.join("report.jsonl");
    let (store, takeover) = setup(&home, HOUR);
    let finished = Arc::new(AtomicUsize::new(0));
    let threads: Vec<_> = (0..WORKERS)
        .map(|worker| {
            let report = report.clone();
            let finished = finished.clone();
            thread::spawn(move || {
                let engine = engine(&report);
                emit(&engine, &worker.to_string(), 0..EVENTS);
                finished.fetch_add(1, Ordering::SeqCst);
            })
        })
        .collect();
    while finished.load(Ordering::SeqCst) < WORKERS {
        takeover.cycle().expect("cycle");
    }
    for thread in threads {
        thread.join().expect("worker");
    }
    takeover.cycle().expect("cycle");
    takeover.cycle().expect("cycle");

    let stored = stored(store.as_ref());
    assert_eq!(stored.len(), WORKERS * EVENTS);
    assert_eq!(unique(&stored), WORKERS * EVENTS);
}

#[test]
fn resume() {
    let home = temp("resume");
    let report = home.join("report.jsonl");
    let (store, first) = setup(&home, HOUR);
    let writer = engine(&report);
    emit(&writer, "writer", 0..5);
    first.cycle().expect("cycle");
    drop(first);
    emit(&writer, "writer", 5..12);
    let (_, second) = setup(&home, HOUR);
    second.cycle().expect("cycle");

    let stored = stored(store.as_ref());
    assert_eq!(stored.len(), 12);
    assert_eq!(unique(&stored), 12);
}

#[test]
fn idle() {
    let home = temp("idle");
    let report = home.join("report.jsonl");
    let (store, takeover) = setup(&home, Duration::ZERO);
    emit(&engine(&report), "writer", 0..5);
    takeover.cycle().expect("cycle");

    assert_eq!(stored(store.as_ref()).len(), 5);
    assert!(taken(&home).is_empty());
    assert!(!report.exists());
}

#[test]
fn malformed() {
    let home = temp("malformed");
    let report = home.join("report.jsonl");
    let (store, takeover) = setup(&home, HOUR);
    emit(&engine(&report), "writer", 0..2);
    fs::OpenOptions::new()
        .append(true)
        .open(&report)
        .and_then(|mut file| file.write_all(b"{\"not\":\"an atom\"}\npartial"))
        .expect("append");
    takeover.cycle().expect("cycle");

    assert_eq!(stored(store.as_ref()).len(), 2);
    let rejected =
        fs::read_to_string(home.join("rejected").join("concord.jsonl")).expect("rejected");
    assert_eq!(rejected, "{\"not\":\"an atom\"}\n");
}

fn setup(home: &Path, grace: Duration) -> (Arc<dyn Store>, Takeover) {
    let store: Arc<dyn Store> = Arc::new(Segments::open(home.join("store")).expect("store"));
    let source =
        Source::new(&format!("concord={}", home.join("report.jsonl").display())).expect("source");
    let takeover = Takeover::new(vec![source], store.clone(), grace, home.join("rejected"));
    (store, takeover)
}

fn engine(report: &Path) -> Engine {
    let policy = Policy::default().reporter(reporter::Spec::file(report));
    Engine::bootstrap(Config::new(policy)).expect("engine")
}

fn emit(engine: &Engine, writer: &str, range: std::ops::Range<usize>) {
    for sequence in range {
        engine
            .append(
                &Context::empty(),
                Candidate::event(json!({"writer": writer, "sequence": sequence})),
            )
            .expect("append");
    }
}

fn stored(store: &dyn Store) -> Vec<Value> {
    let mut lines = Vec::new();
    store
        .read(&Filter::default(), &mut |line| {
            lines.push(serde_json::from_slice(line).map_err(|error| error.to_string())?);
            Ok(())
        })
        .expect("read");
    lines
}

fn unique(stored: &[Value]) -> usize {
    stored
        .iter()
        .map(|atom| atom["payload"].to_string())
        .collect::<BTreeSet<_>>()
        .len()
}

fn taken(home: &Path) -> Vec<PathBuf> {
    fs::read_dir(home)
        .expect("home")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name.starts_with("report.jsonl.taken.") && !name.ends_with(".offset")
        })
        .collect()
}

fn temp(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("locus-api-{label}-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&path).expect("temp");
    path
}
