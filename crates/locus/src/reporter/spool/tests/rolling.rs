use super::{Spooled, emit, open, temp};
use crate::reporter::spool::claim;
use std::fs;

#[test]
fn ledger() {
    let directory = temp("ledger");
    let ledger = directory.join("loss.jsonl");
    fs::File::create(&ledger)
        .unwrap()
        .set_len(1024 * 1024)
        .unwrap();
    let spool = open(directory.to_str().unwrap(), 1024, 512).unwrap();
    let detail = (0..100)
        .find_map(|sequence| emit(&spool, "producer", sequence))
        .expect("producer overflow diagnostic");
    assert!(detail.contains("spool over ceiling dropped"));
    let entries = Spooled(&directory).ledger();
    assert_eq!(entries.len(), 1);
    assert!(entries[0]["records"].as_u64().unwrap() > 0);
    assert!(fs::metadata(ledger).unwrap().len() <= 1024 * 1024);
}

#[test]
fn private() {
    let directory = temp("private");
    let spec = crate::reporter::Spec::local(&*directory, 2);
    let spool = crate::reporter::build(&spec, Some("local")).unwrap();
    assert!(spool.target().is_none());
    assert!(claim(&directory).is_err());
    let api = crate::reporter::Spec::api("http://127.0.0.1:0", &*directory);
    assert!(crate::reporter::build(&api, Some("producer")).is_err());
}
