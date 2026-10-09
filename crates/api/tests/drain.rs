use locus::reporter;
use locus::{Candidate, Config, Context, Engine, Policy};
use locus_api::drain::Drain;
use locus_api::registry::Registry;
use locus_api::store::{Filter, Segments, Store};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const CLOSED: &str = "http://127.0.0.1:9";

static STAMP: AtomicU64 = AtomicU64::new(1);

struct Spool(PathBuf);

#[test]
fn drained() {
    let home = temp("drained");
    let spool = Spool(home.join("spool"));
    let (store, drain) = setup(&home, &spool);
    spool.emit(0..100);
    spool.seal();
    drain.cycle().expect("cycle");
    spool.emit(100..200);
    spool.seal();
    spool.emit(200..210);
    drain.cycle().expect("cycle");

    let stored = stored(store.as_ref());
    assert_eq!(stored.len(), 200);
    let ids: BTreeSet<_> = stored.iter().map(|atom| atom["id"].to_string()).collect();
    assert_eq!(ids.len(), 200);
    assert!(stored.iter().all(|atom| atom["producer"] == "concord"));
    assert_eq!(
        spool.names(),
        BTreeSet::from(["active.jsonl".to_string(), "spool.lock".to_string()])
    );
}

#[test]
fn resume() {
    let home = temp("resume");
    let spool = Spool(home.join("spool"));
    let (store, drain) = setup(&home, &spool);
    spool.emit(0..12);
    let sealed = spool.seal();
    let claimed = sealed.replace("sealed-", "claimed-");
    let root = &spool.0;
    fs::rename(root.join(&sealed), root.join(&claimed)).expect("claim");
    let text = fs::read_to_string(root.join(&claimed)).expect("claimed");
    let offset: usize = text.lines().take(5).map(|line| line.len() + 1).sum();
    fs::write(root.join(format!("{claimed}.offset")), offset.to_string()).expect("offset");
    drain.cycle().expect("cycle");

    let stored = stored(store.as_ref());
    assert_eq!(stored.len(), 7);
    assert_eq!(stored[0]["payload"]["sequence"], 5);
    assert_eq!(spool.names(), BTreeSet::from(["spool.lock".to_string()]));
}

#[test]
fn malformed() {
    let home = temp("malformed");
    let spool = Spool(home.join("spool"));
    let (store, drain) = setup(&home, &spool);
    spool.emit(0..2);
    fs::OpenOptions::new()
        .append(true)
        .open(spool.0.join("active.jsonl"))
        .and_then(|mut file| file.write_all(b"{\"not\":\"an atom\"}\n"))
        .expect("append");
    spool.seal();
    drain.cycle().expect("cycle");

    assert_eq!(stored(store.as_ref()).len(), 2);
    let rejected =
        fs::read_to_string(home.join("rejected").join("concord.jsonl")).expect("rejected");
    assert_eq!(rejected, "{\"not\":\"an atom\"}\n");
}

fn setup(home: &Path, spool: &Spool) -> (Arc<dyn Store>, Drain) {
    drop(spool.engine());
    let registry = Arc::new(Registry::open(home.join("spools.json")).expect("registry"));
    registry.enroll("concord", &spool.0).expect("enroll");
    let store: Arc<dyn Store> = Arc::new(Segments::open(home.join("store")).expect("store"));
    (
        store.clone(),
        Drain::new(store, home.join("rejected"), registry),
    )
}

impl Spool {
    fn engine(&self) -> Engine {
        let policy = Policy::default()
            .producer("concord")
            .reporter(reporter::Spec::api(CLOSED, &self.0));
        Engine::bootstrap(Config::new(policy)).expect("engine")
    }

    fn emit(&self, range: std::ops::Range<usize>) {
        let engine = self.engine();
        for sequence in range {
            engine
                .append(
                    &Context::empty(),
                    Candidate::event(json!({"sequence": sequence})),
                )
                .expect("append");
        }
    }

    fn seal(&self) -> String {
        let name = format!("sealed-{:020}.jsonl", STAMP.fetch_add(1, Ordering::SeqCst));
        fs::rename(self.0.join("active.jsonl"), self.0.join(&name)).expect("seal");
        name
    }

    fn names(&self) -> BTreeSet<String> {
        fs::read_dir(&self.0)
            .expect("spool")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .into_string()
                    .expect("name")
            })
            .filter(|name| name != "registration")
            .collect()
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
