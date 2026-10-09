#[path = "inspect/fixture.rs"]
mod fixture;

use fixture::{Root, TIMED, framed, records, run};

#[test]
fn held() {
    let root = Root::new(Some(TIMED));
    let input = [
        framed(0, "outer", "enter"),
        framed(10, "inner", "enter"),
        framed(20, "inner", "return"),
        framed(30, "outer", "return"),
    ]
    .join("\n");
    let output = run(&root, &input);
    assert_eq!(output.status.code(), Some(1));
    let records = records(&output);
    assert_eq!(records[0]["analyzer"], "trace.held");
    assert_eq!(records[0]["group"]["role"], "locus.trace");
    assert_eq!(records[0]["group"]["key"], "trace");
    assert_eq!(
        records[0]["measurement"]["mapping"],
        "held-time-nanoseconds"
    );
    assert_eq!(records[0]["measurement"]["value"], 30);
    assert_eq!(records[0]["measurement"]["inclusive"], 40);
    assert_eq!(records[0]["measurement"]["spans"], 2);
    assert_eq!(records[1]["findings"], 1);
    assert_eq!(records[1]["refused"], 0);
}

#[test]
fn under() {
    let root = Root::new(Some(TIMED));
    let input = [framed(0, "brief", "enter"), framed(4, "brief", "return")].join("\n");
    let output = run(&root, &input);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(records(&output)[0]["findings"], 0);
}

#[test]
fn refused() {
    let root = Root::new(Some(TIMED));
    let input = [
        framed(0, "outer", "enter"),
        framed(10, "inner", "enter"),
        framed(20, "outer", "return"),
        framed(30, "inner", "return"),
        framed(100, "clean", "enter"),
        framed(130, "clean", "return"),
    ]
    .join("\n");
    let output = run(&root, &input);
    assert_eq!(output.status.code(), Some(1));
    let records = records(&output);
    assert_eq!(records[0]["measurement"]["value"], 30);
    assert_eq!(records[0]["measurement"]["spans"], 1);
    assert_eq!(records[1]["refused"], 2);
}

#[test]
fn headless() {
    let root = Root::new(Some(TIMED));
    let input = [
        framed(5, "inner", "enter"),
        framed(15, "inner", "return"),
        framed(20, "outer", "return"),
    ]
    .join("\n");
    let output = run(&root, &input);
    let records = records(&output);
    let summary = records.last().expect("summary");
    assert_eq!(summary["headless"], 1);
    assert_eq!(summary["refused"], 0);
    assert_eq!(records[0]["measurement"]["value"], 10);
    assert_eq!(records[0]["measurement"]["spans"], 1);
}
