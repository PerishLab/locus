use super::{Report, Spool, claim, open, segments};
use crate::Atom;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CHILD: &str = "LOCUS_SPOOL_CHILD";
const WORKER: &str = "LOCUS_SPOOL_WORKER";

#[test]
fn writer() {
    let (Ok(directory), Ok(worker)) = (std::env::var(CHILD), std::env::var(WORKER)) else {
        return;
    };
    let spool = open(&directory, 1 << 30, 4_096).expect("spool");
    for sequence in 0..300 {
        emit(&spool, &worker, sequence);
    }
}

#[test]
fn processes() {
    let directory = temp("processes");
    let children: Vec<_> = (0..6)
        .map(|worker| {
            Command::new(std::env::current_exe().expect("test binary"))
                .args(["--exact", "reporter::spool::tests::writer", "--quiet"])
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
    let sealed = segments(&directory).expect("segments");
    assert!(sealed.len() > 10);
    for path in sealed {
        let size = fs::metadata(&path).expect("segment").len();
        assert!((4_096..4_096 + 1_024).contains(&size), "{size}");
    }
}

#[test]
fn ceiling() {
    let directory = temp("ceiling");
    let spool = open(directory.to_str().expect("utf8"), 8_192, 2_048).expect("spool");
    let mut losses = 0;
    for sequence in 0..300 {
        losses += usize::from(emit(&spool, "single", sequence).is_some());
        assert!(Spooled(&directory).footprint() <= 8_192);
    }
    let lost: u64 = Spooled(&directory)
        .ledger()
        .iter()
        .map(|entry| entry["records"].as_u64().expect("records"))
        .sum();
    assert!(lost > 0 && losses > 0);
    assert_eq!(lost + Spooled(&directory).atoms().len() as u64, 300);
}

#[test]
fn consumer() {
    let directory = temp("consumer");
    let spool = open(directory.to_str().expect("utf8"), 8_192, 2_048).expect("spool");
    let mut sequence = 0;
    while segments(&directory).expect("segments").len() < 2 {
        emit(&spool, "single", sequence);
        sequence += 1;
    }
    let claimed = claim(&directory).expect("claim");
    assert_eq!(claimed.len(), 2);
    assert!(segments(&directory).expect("segments").is_empty());
    let held: Vec<_> = claimed
        .iter()
        .map(|path| fs::read(path).expect("claimed"))
        .collect();
    for _ in 0..200 {
        emit(&spool, "single", sequence);
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
    let spool = open(directory.to_str().expect("utf8"), 1 << 30, 1_024).expect("spool");
    for sequence in 0..40 {
        emit(&spool, "single", sequence);
    }
    let before: Vec<_> = segments(&directory)
        .expect("segments")
        .into_iter()
        .map(|path| (fs::read(&path).expect("segment"), path))
        .collect();
    for sequence in 40..80 {
        emit(&spool, "single", sequence);
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
fn inherited() {
    let directory = temp("inherited");
    let line = format!("{}\n", json!({"at": 1, "choices": [], "context": {}}));
    fs::write(
        directory.join("sealed-00000000000000000001.jsonl"),
        line.repeat(300),
    )
    .expect("seed");
    let spool = open(directory.to_str().expect("utf8"), 8_192, 2_048).expect("spool");
    emit(&spool, "single", 0);
    assert!(Spooled(&directory).footprint() <= 8_192);
    assert_eq!(Spooled(&directory).ledger()[0]["records"], 300);
}

fn emit(spool: &Spool, worker: &str, sequence: usize) -> Option<String> {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let atom: Atom = serde_json::from_value(json!({
        "at": u64::try_from(at).expect("at"),
        "id": format!("{worker}-{sequence}"),
        "producer": "concord",
        "context": {},
        "choices": [],
        "payload": {"worker": worker, "sequence": sequence},
    }))
    .expect("atom");
    spool.report(&atom).expect("report")
}

struct Spooled<'a>(&'a Path);

impl Spooled<'_> {
    fn files(&self) -> Vec<PathBuf> {
        let mut paths = segments(self.0).expect("segments");
        paths.push(self.0.join("active.jsonl"));
        paths
    }

    fn atoms(&self) -> Vec<Value> {
        self.files()
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
        fs::read_to_string(self.0.join("loss.jsonl"))
            .map(|text| {
                text.lines()
                    .map(|line| serde_json::from_str(line).expect("entry"))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn footprint(&self) -> u64 {
        self.files()
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
