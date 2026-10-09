use locus::reporter;
use locus::{Candidate, Config, Context, Engine, Policy};
use locus_api::drain::Drain;
use locus_api::registry::Registry;
use locus_api::retention::Retention;
use locus_api::server::{self, Shared};
use locus_api::store::{Filter, Segments, Store};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const CLOSED: &str = "http://127.0.0.1:9";

#[test]
fn persisted() {
    let home = temp("persisted");
    let spool = home.join("spool");
    drop(engine(reporter::Spec::api(CLOSED, &spool)));
    let registry = Registry::open(home.join("spools.json")).expect("registry");
    assert!(registry.enroll("concord", &home).is_err());
    assert!(registry.enroll("Concord", &spool).is_err());
    registry.enroll("concord", &spool).expect("enroll");
    registry.enroll("concord", &spool).expect("idempotent");
    let reopened = Registry::open(home.join("spools.json")).expect("reopen");
    let sources = reopened.sources();
    assert_eq!(sources.len(), 1);
    assert_eq!(
        sources[0].path,
        fs::canonicalize(&spool).expect("canonical")
    );
}

#[test]
fn drained() {
    let home = temp("drained");
    let spool = home.join("spool");
    let engine = engine(reporter::Spec::api(CLOSED, &spool));
    for sequence in 0..100 {
        emit(&engine, sequence);
    }
    fs::rename(
        spool.join("active.jsonl"),
        spool.join("sealed-00000000000000000001.jsonl"),
    )
    .expect("seal");
    for sequence in 100..110 {
        emit(&engine, sequence);
    }
    let registry = Arc::new(Registry::open(home.join("spools.json")).expect("registry"));
    registry.enroll("concord", &spool).expect("enroll");
    let store: Arc<dyn Store> = Arc::new(Segments::open(home.join("store")).expect("store"));
    Drain::new(store.clone(), home.join("rejected"), registry)
        .cycle()
        .expect("cycle");
    let mut stored = 0;
    store
        .read(&Filter::default(), &mut |_| {
            stored += 1;
            Ok(())
        })
        .expect("read");
    let active = fs::read_to_string(spool.join("active.jsonl")).expect("active");
    assert_eq!(stored, 100);
    assert_eq!(active.lines().count(), 10);
}

#[test]
fn served() {
    let home = temp("served");
    let registry = Arc::new(Registry::open(home.join("spools.json")).expect("registry"));
    let store: Arc<dyn Store> = Arc::new(Segments::open(home.join("store")).expect("store"));
    let (sender, receiver) = mpsc::channel();
    let shared = registry.clone();
    thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("listen");
            sender
                .send(listener.local_addr().expect("address"))
                .expect("send");
            server::serve(
                listener,
                Shared::new(store, shared, Retention::default()),
                std::future::pending(),
            )
            .await
            .expect("serve");
        });
    });
    let endpoint = format!("http://{}", receiver.recv().expect("address"));
    let spool = home.join("spool");
    let engine = engine(reporter::Spec::api(endpoint, &spool));
    emit(&engine, 0);
    let sources = registry.sources();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].producer.text(), "concord");
    assert_eq!(
        sources[0].path,
        fs::canonicalize(&spool).expect("canonical")
    );
}

fn engine(spec: reporter::Spec) -> Engine {
    let policy = Policy::default().producer("concord").reporter(spec);
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
    let path: PathBuf = std::env::temp_dir().join(format!(
        "locus-registry-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(Path::new(&path)).expect("temp");
    path
}
