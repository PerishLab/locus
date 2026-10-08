use locus::{Key, Role};
use locus_api::store::{Filter, Producer, Record, Segments, Store, Window};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const DAY: u64 = 86_400_000_000_000;
const BASE: u64 = 20_369 * DAY;

#[test]
fn verbatim() {
    let store = store("verbatim");
    let line = format!(
        "{{\"at\":{},\"context\":{{\"locus.trace\":\"a\"}},\"choices\":[],\"payload\":{{\"z\":1,\"a\":2}}}}",
        BASE
    );
    store
        .append(
            &producer("concord"),
            &[Record::parse(line.as_bytes()).expect("record")],
        )
        .expect("append");
    assert_eq!(read(&store, &Filter::default()), vec![line]);
}

#[test]
fn window() {
    let store = store("window");
    append(
        &store,
        "concord",
        &[(BASE - 1, "a"), (BASE, "a"), (BASE + DAY, "b")],
    );
    let filter = Filter {
        window: Window {
            from: Some(BASE),
            to: Some(BASE + DAY),
        },
        ..Filter::default()
    };
    assert_eq!(times(&read(&store, &filter)), vec![BASE]);
    assert_eq!(
        times(&read(&store, &Filter::default())),
        vec![BASE - 1, BASE, BASE + DAY]
    );
}

#[test]
fn selector() {
    let store = store("selector");
    append(
        &store,
        "concord",
        &[(BASE, "a"), (BASE + 1, "b"), (BASE + 2, "a")],
    );
    let filter = Filter {
        selector: Some((Role::trace(), Key::new("a").expect("key"))),
        ..Filter::default()
    };
    assert_eq!(times(&read(&store, &filter)), vec![BASE, BASE + 2]);
}

#[test]
fn producers() {
    let store = store("producers");
    append(&store, "concord", &[(BASE, "a")]);
    append(&store, "santi", &[(BASE + 1, "b")]);
    let filter = Filter {
        producer: Some(producer("santi")),
        ..Filter::default()
    };
    assert_eq!(times(&read(&store, &filter)), vec![BASE + 1]);
    assert_eq!(read(&store, &Filter::default()).len(), 2);
}

#[test]
fn expire() {
    let store = store("expire");
    append(&store, "concord", &[(BASE, "a"), (BASE + DAY, "b")]);
    store
        .expire(&producer("concord"), BASE / DAY)
        .expect("expire");
    assert_eq!(times(&read(&store, &Filter::default())), vec![BASE + DAY]);
    store
        .expire(&producer("concord"), BASE / DAY)
        .expect("idempotent");
}

#[test]
fn names() {
    for invalid in ["", "Concord", "-concord", "a/b", "a b"] {
        assert!(Producer::new(invalid).is_err(), "{invalid:?}");
    }
    assert!(Producer::new("concord-2.dev").is_ok());
}

fn store(label: &str) -> Segments {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path: PathBuf = std::env::temp_dir().join(format!(
        "locus-store-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("temp");
    Segments::open(path).expect("store")
}

fn producer(name: &str) -> Producer {
    Producer::new(name).expect("producer")
}

fn append(store: &Segments, name: &str, atoms: &[(u64, &str)]) {
    let records: Vec<Record> = atoms
        .iter()
        .map(|(at, trace)| {
            let line = format!(
                "{{\"at\":{at},\"context\":{{\"locus.trace\":\"{trace}\"}},\"choices\":[]}}"
            );
            Record::parse(line.as_bytes()).expect("record")
        })
        .collect();
    store.append(&producer(name), &records).expect("append");
}

fn read(store: &Segments, filter: &Filter) -> Vec<String> {
    let mut lines = Vec::new();
    store
        .read(filter, &mut |line| {
            lines.push(String::from_utf8_lossy(line).trim_end().to_string());
            Ok(())
        })
        .expect("read");
    lines
}

fn times(lines: &[String]) -> Vec<u64> {
    lines
        .iter()
        .map(|line| {
            let atom: serde_json::Value = serde_json::from_str(line).expect("atom");
            atom["at"].as_u64().expect("at")
        })
        .collect()
}
