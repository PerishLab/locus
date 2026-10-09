#[path = "inspect/fixture.rs"]
mod fixture;

use fixture::{CONFIG, Root, TIMED, stub};
use locus::reporter;
use locus::{Candidate, Config, Context, Engine, Policy, Role};
use locus_api::drain::Drain;
use locus_api::registry::Registry;
use locus_api::server;
use locus_api::store::{Segments, Store};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

const CLOSED: &str = "http://127.0.0.1:9";

type View<'a> = Option<(&'a Engine, &'a Context)>;

struct Served {
    api: String,
    sample: String,
    trace: String,
}

#[test]
fn identical() {
    let served = launch();
    let cases: [&[&str]; 4] = [
        &["span"],
        &["query", "locus.trace"],
        &["query", "locus.trace", &served.trace],
        &["query", "locus.span"],
    ];
    for args in cases {
        compare(None, args, &served);
    }
    for config in [CONFIG, TIMED] {
        let root = Root::new(Some(config));
        compare(Some(root.path()), &["inspect"], &served);
    }
}

#[test]
fn selected() {
    let served = launch();
    let selector = format!("locus.trace={}", served.trace);
    let args = [
        "query",
        "locus.trace",
        "--api",
        &served.api,
        "--select",
        &selector,
    ];
    let output = locus(None, &args);
    let expected = served
        .sample
        .lines()
        .filter(|line| line.contains(&served.trace))
        .count();
    let summary = String::from_utf8(output.stdout).expect("utf8");
    assert!(
        summary.contains(&format!("\"records\":{expected},")),
        "{summary}"
    );
}

#[test]
fn refused() {
    let served = launch();
    let args = ["span", "--api", &served.api, "--select", "locus.trace"];
    let output = locus(None, &args);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[locus::trace(with = view)]
fn command(view: View<'_>, depth: usize) {
    if depth > 0 {
        command(view, depth - 1);
        command(view, depth - 1);
    }
}

fn compare(root: Option<&Path>, args: &[&str], served: &Served) {
    let local = locus(
        root,
        &[args, &["--api", &stub::serve(&served.sample)]].concat(),
    );
    let remote = locus(root, &[args, &["--api", &served.api]].concat());
    assert!(!local.stdout.is_empty(), "{args:?}");
    assert!(local.stderr.is_empty(), "{args:?}");
    assert_eq!(local.status.code(), remote.status.code(), "{args:?}");
    assert_eq!(local.stdout, remote.stdout, "{args:?}");
    assert_eq!(local.stderr, remote.stderr, "{args:?}");
}

fn locus(root: Option<&Path>, args: &[&str]) -> Output {
    let mut args = args.to_vec();
    let root = root.map(|root| root.display().to_string());
    if let Some(root) = &root {
        args.insert(1, root);
    }
    Command::new(env!("CARGO_BIN_EXE_locus"))
        .args(&args)
        .output()
        .expect("locus")
}

fn record(spool: &Path) {
    let policy = Policy::default()
        .producer("concord")
        .reporter(reporter::Spec::api(CLOSED, spool));
    let engine = Engine::bootstrap(Config::new(policy)).expect("engine");
    for run in 0..3 {
        let root = engine
            .append(
                &Context::empty(),
                Candidate::event(json!({"event": "cli.start", "run": run})).ensure(Role::trace()),
            )
            .expect("start")
            .context();
        command(Some((&engine, &root)), 3);
        engine
            .append(
                &root,
                Candidate::event(json!({"event": "cli.finish", "run": run})),
            )
            .expect("finish");
    }
}

fn launch() -> Served {
    let home = Root::new(None);
    let spool = home.path().join("spool");
    record(&spool);
    let sealed = spool.join("sealed-00000000000000000001.jsonl");
    fs::rename(spool.join("active.jsonl"), &sealed).expect("seal");
    let sample = fs::read_to_string(&sealed).expect("sample");
    let trace = first(&sample);
    let store: Arc<dyn Store> = Arc::new(Segments::open(home.path().join("store")).expect("store"));
    let registry = Arc::new(Registry::open(home.path().join("spools.json")).expect("registry"));
    registry.enroll("concord", &spool).expect("enroll");
    Drain::new(
        store.clone(),
        home.path().join("rejected"),
        registry.clone(),
    )
    .cycle()
    .expect("drain");
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _home = home;
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("listen");
            sender
                .send(listener.local_addr().expect("address"))
                .expect("send");
            server::serve(listener, store, registry, std::future::pending())
                .await
                .expect("serve");
        });
    });
    let api = format!("http://{}", receiver.recv().expect("address"));
    Served { api, sample, trace }
}

fn first(sample: &str) -> String {
    let line = sample.lines().next().expect("line");
    let atom: serde_json::Value = serde_json::from_str(line).expect("atom");
    atom["context"]["locus.trace"]
        .as_str()
        .expect("trace")
        .to_string()
}
