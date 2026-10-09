use locus_api::registry::Registry;
use locus_api::retention::Retention;
use locus_api::server::{self, Shared};
use locus_api::store::{Filter, Producer, Record, Segments, Store};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const DAY: u64 = 86_400_000_000_000;
const TODAY: u64 = 20_000;

#[test]
fn dropped() {
    let store = seeded("dropped");
    Retention::days(2)
        .apply(&store, TODAY * DAY + 5)
        .expect("apply");
    assert_eq!(days(&store), vec![TODAY - 2, TODAY - 1, TODAY]);
    assert_eq!(count(&store), 3);
}

#[test]
fn undeclared() {
    let store = seeded("undeclared");
    Retention::default()
        .apply(&store, TODAY * DAY)
        .expect("apply");
    assert_eq!(days(&store).len(), 5);
    assert_eq!(Retention::default().floor(TODAY * DAY), None);
    assert_eq!(
        Retention::days(2).floor(TODAY * DAY + 5),
        Some((TODAY - 2) * DAY)
    );
}

#[test]
fn declared() {
    let home = temp("declared");
    assert_eq!(
        Retention::read(&home).expect("absent").floor(TODAY * DAY),
        None
    );
    fs::write(home.join("locus-api.toml"), "[retention]\ndays = 3\n").expect("config");
    assert_eq!(
        Retention::read(&home).expect("declared").floor(TODAY * DAY),
        Some((TODAY - 3) * DAY)
    );
    for malformed in [
        "[retention]\ndays = -1\n",
        "[retention]\nweeks = 1\n",
        "spool = 1\n",
    ] {
        fs::write(home.join("locus-api.toml"), malformed).expect("config");
        let error = Retention::read(&home).expect_err("malformed refuses");
        assert!(error.contains("locus-api.toml"), "{error}");
    }
}

#[test]
fn reported() {
    let floor = header(Retention::days(7));
    let expected = Retention::days(7).floor(now()).expect("floor");
    assert_eq!(floor, Some(expected));
    assert_eq!(header(Retention::default()), None);
}

fn seeded(label: &str) -> Segments {
    let store = Segments::open(temp(label).join("store")).expect("store");
    let producer = Producer::new("concord").expect("producer");
    let records: Vec<Record> = (0..5)
        .map(|back| {
            let at = (TODAY - 4 + back) * DAY + 1;
            Record::parse(format!(r#"{{"at":{at},"choices":[],"context":{{}}}}"#).as_bytes())
                .expect("record")
        })
        .collect();
    store.append(&producer, &records, "seed").expect("append");
    store
}

fn days(store: &Segments) -> Vec<u64> {
    let mut found = Vec::new();
    store
        .read(&Filter::default(), &mut |line| {
            let atom: serde_json::Value = serde_json::from_slice(line).expect("atom");
            found.push(atom["at"].as_u64().expect("at") / DAY);
            Ok(())
        })
        .expect("read");
    found.dedup();
    found
}

fn count(store: &Segments) -> usize {
    let mut lines = 0;
    store
        .read(&Filter::default(), &mut |_| {
            lines += 1;
            Ok(())
        })
        .expect("read");
    lines
}

fn header(retention: Retention) -> Option<u64> {
    let home = temp("header");
    let store: Arc<dyn Store> = Arc::new(Segments::open(home.join("store")).expect("store"));
    let registry = Arc::new(Registry::open(home.join("spools.json")).expect("registry"));
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("listen");
            sender
                .send(listener.local_addr().expect("address"))
                .expect("send");
            let shared = Shared::new(store, registry, retention);
            server::serve(listener, shared, std::future::pending())
                .await
                .expect("serve");
        });
    });
    let address = receiver.recv().expect("address");
    let mut stream = TcpStream::connect(address).expect("connect");
    stream
        .write_all(b"GET /api/v1/atoms HTTP/1.1\r\nhost: locus\r\nconnection: close\r\n\r\n")
        .expect("request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("response");
    response
        .lines()
        .take_while(|line| !line.is_empty())
        .find_map(|line| line.strip_prefix("locus-retained-from: "))
        .map(|value| value.trim().parse().expect("instant"))
}

fn now() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos(),
    )
    .expect("instant")
}

fn temp(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "locus-retention-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("temp");
    path
}
