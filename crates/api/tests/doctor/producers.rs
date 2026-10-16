use super::{Temp, detail, doctor, healthy, next, offline, status};
use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, SystemTime};

fn expect(home: &Temp, names: &str) {
    fs::write(
        home.path().join("locus-api.toml"),
        format!("[producers]\nexpected = [{names}]\n"),
    )
    .expect("declared");
}

fn touch(home: &Temp, name: &str, age: Duration) {
    let path = home.path().join("spool").join(name);
    fs::write(&path, b"").expect("write");
    File::options()
        .write(true)
        .open(&path)
        .and_then(|file| file.set_modified(SystemTime::now() - age))
        .expect("age");
}

fn lines(home: &Path) -> (String, String) {
    let report = doctor(home, offline());
    (status(&report, "producers"), detail(&report, "producers"))
}

#[test]
fn absent() {
    let home = healthy("absent");
    expect(&home, "\"concord\", \"plumb\"");
    let report = doctor(home.path(), offline());
    assert_eq!(status(&report, "producers"), "warn", "{}", report.json);
    assert_eq!(status(&report, "config"), "ok", "{}", report.json);
    let text = detail(&report, "producers");
    assert!(text.contains("plumb never registered"), "{text}");
    assert!(
        text.contains("concord registered but has written nothing"),
        "{text}"
    );
    assert!(next(&report, "producers").contains("enabled = true"));
}

#[test]
fn fresh() {
    let home = healthy("fresh");
    expect(&home, "\"concord\"");
    touch(&home, "registration", Duration::from_secs(30));
    touch(&home, "spool.lock", Duration::ZERO);
    let (state, text) = lines(home.path());
    assert_eq!(state, "ok", "{text}");
    assert!(text.contains("concord last wrote"), "{text}");
}

#[test]
fn silent() {
    let home = healthy("silent");
    touch(&home, "registration", Duration::from_secs(25 * 60 * 60));
    touch(&home, "spool.lock", Duration::ZERO);
    let (state, text) = lines(home.path());
    assert_eq!(state, "warn", "{text}");
    assert!(text.contains("concord silent for 25h"), "{text}");
    touch(&home, "active.jsonl", Duration::from_secs(5));
    let (state, text) = lines(home.path());
    assert_eq!(state, "ok", "{text}");
}

#[test]
fn undeclared() {
    let home = healthy("undeclared");
    touch(
        &home,
        "sealed-00000000000000000001.jsonl",
        Duration::from_secs(600),
    );
    let (state, text) = lines(home.path());
    assert_eq!(state, "ok", "{text}");
    assert_eq!(text, "concord last wrote 10m ago");
}

#[test]
fn invalid() {
    let home = healthy("invalid");
    for names in ["\"Bad Name\"", "\"plumb\", \"plumb\""] {
        expect(&home, names);
        let report = doctor(home.path(), offline());
        assert_eq!(status(&report, "producers"), "warn", "{names}");
        assert!(
            next(&report, "producers").contains("[producers]"),
            "{names}"
        );
    }
}
