use locus::{Key, Role};
use locus_api::column::Columns;
use locus_api::migrate;
use locus_api::registry::Registry;
use locus_api::retention::Retention;
use locus_api::server::{self, Shared};
use locus_api::store::{Filter, Producer, Record, Segments, Store};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;

const DAY: u64 = 86_400_000_000_000;
const BASE: u64 = 20_000 * DAY;
const FAR: u64 = u64::MAX / 2;

struct Opened {
    _runtime: Runtime,
    home: PathBuf,
    columns: Columns,
}

impl Opened {
    fn new(label: &str) -> Self {
        let home = temp(label);
        let runtime = Runtime::new().expect("runtime");
        let columns = runtime
            .block_on(Columns::open(&home, runtime.handle().clone()))
            .expect("columns");
        Self {
            _runtime: runtime,
            home,
            columns,
        }
    }

    fn lines(&self, filter: &Filter) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        self.columns
            .read(filter, &mut |line| {
                found.insert(String::from_utf8(line.to_vec()).expect("utf8"));
                Ok(())
            })
            .expect("read");
        found
    }
}

#[test]
fn verbatim() {
    let opened = Opened::new("verbatim");
    let sealed = atoms(0..6);
    let open = atoms(6..9);
    put(&opened.columns, "concord", &sealed, "one");
    opened.columns.tick(FAR).expect("seal");
    put(&opened.columns, "concord", &open, "two");
    let all: BTreeSet<String> = sealed
        .iter()
        .chain(&open)
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(opened.lines(&Filter::default()), all);
    let traced = Filter {
        selector: Some((Role::trace(), Key::new("t1").expect("key"))),
        ..Filter::default()
    };
    assert!(
        opened
            .lines(&traced)
            .iter()
            .all(|line| line.contains(r#""locus.trace":"t1""#))
    );
    assert_eq!(opened.lines(&traced).len(), 3);
    let spanned = Filter {
        selector: Some((
            Role::new("locus.span").expect("role"),
            Key::new("s4").expect("key"),
        )),
        ..Filter::default()
    };
    assert_eq!(opened.lines(&spanned).len(), 1);
}

#[test]
fn idempotent() {
    let opened = Opened::new("idempotent");
    let lines = atoms(0..3);
    put(&opened.columns, "concord", &lines, "same");
    opened.columns.tick(FAR).expect("seal");
    put(&opened.columns, "concord", &lines, "same");
    assert_eq!(opened.lines(&Filter::default()).len(), 3);
}

#[test]
fn retained() {
    let opened = Opened::new("retained");
    put(&opened.columns, "concord", &[atom(BASE, 0)], "old");
    put(&opened.columns, "concord", &[atom(BASE + DAY, 1)], "new");
    opened.columns.tick(FAR).expect("seal");
    opened.columns.retain(BASE / DAY + 1, 0).expect("retain");
    let left = opened.lines(&Filter::default());
    assert_eq!(left.len(), 1);
    assert!(
        left.iter()
            .all(|line| line.contains(&format!("\"at\":{}", BASE + DAY)))
    );
}

#[test]
fn migrated() {
    let opened = Opened::new("migrated");
    let legacy = opened.home.join("store");
    let segments = Segments::open(&legacy).expect("legacy");
    let mut expected = BTreeSet::new();
    for (producer, offset) in [("concord", 0), ("santi", 1)] {
        let lines: Vec<String> = (0..4)
            .map(|index| atom(BASE + index * DAY / 2 + offset, index))
            .collect();
        put(&segments, producer, &lines, "legacy");
        expected.extend(lines.iter().map(|line| format!("{line}\n")));
    }
    assert_eq!(
        migrate::migrate(&legacy, &opened.columns).expect("migrate"),
        4
    );
    assert!(!legacy.exists());
    assert_eq!(opened.lines(&Filter::default()), expected);
}

#[test]
fn served() {
    let opened = Opened::new("served");
    let lines: Vec<String> = (0..200)
        .map(|index| {
            atom(BASE + index, index).replace(
                r#""choices":[]"#,
                &format!(r#""choices":[],"payload":"{}""#, "x".repeat(1_000)),
            )
        })
        .collect();
    put(&opened.columns, "concord", &lines[..3], "one");
    opened.columns.tick(FAR).expect("seal");
    put(&opened.columns, "concord", &lines[3..], "two");
    let registry = Arc::new(Registry::open(opened.home.join("spools.json")).expect("registry"));
    let Opened {
        _runtime: runtime,
        columns,
        ..
    } = opened;
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .expect("listen");
    let address = listener.local_addr().expect("address");
    let shared = Shared::new(Arc::new(columns), registry, Retention::default());
    runtime.spawn(server::serve(listener, shared, std::future::pending()));
    let mut stream = TcpStream::connect(address).expect("connect");
    stream
        .write_all(b"GET /api/v1/atoms HTTP/1.0\r\nhost: locus\r\n\r\n")
        .expect("request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("response");
    let body = response.split("\r\n\r\n").nth(1).expect("body");
    let served: BTreeSet<&str> = body.lines().filter(|line| line.starts_with('{')).collect();
    let expected: BTreeSet<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(served, expected);
}

fn put(store: &dyn Store, producer: &str, lines: &[String], token: &str) {
    let records: Vec<Record> = lines
        .iter()
        .map(|line| Record::parse(line.as_bytes()).expect("record"))
        .collect();
    store
        .append(&Producer::new(producer).expect("producer"), &records, token)
        .expect("append");
}

fn atoms(range: std::ops::Range<u64>) -> Vec<String> {
    range.map(|index| atom(BASE + index, index)).collect()
}

fn atom(at: u64, index: u64) -> String {
    format!(
        r#"{{"at":{at},"id":"i{index}","producer":"concord","context":{{"locus.trace":"t{}","locus.span":"s{index}"}},"choices":[]}}"#,
        index % 3
    )
}

fn temp(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "locus-column-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("temp");
    path
}
