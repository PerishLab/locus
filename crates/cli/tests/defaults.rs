#[path = "inspect/fixture.rs"]
mod fixture;

use fixture::{DEFAULTS, Root, framed, records, run, stub};
use serde_json::json;
use std::process::Command;

const SECOND: u64 = 1_000_000_000;

fn refusal(config: &str) -> String {
    let root = Root::new(Some(config));
    let output = run(&root, "");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn held() {
    let root = Root::new(None);
    let input = [
        framed(0, "slow", "enter"),
        framed(6 * SECOND, "slow", "return"),
    ]
    .join("\n");
    let output = run(&root, &input);
    assert_eq!(output.status.code(), Some(1));
    let records = records(&output);
    assert_eq!(records[0]["analyzer"], "span.held");
    assert_eq!(records[0]["group"]["key"], "slow");
    assert_eq!(records[0]["threshold"]["above"], 5 * SECOND);
    assert_eq!(records[1]["findings"], 1);
}

#[test]
fn overridden() {
    let config = r#"
version = 1

[[analyzer]]
id = "span.held"
mapping = { kind = "held-time-nanoseconds", group = "locus.span" }
threshold = { above = 10000000000 }
"#;
    let root = Root::new(Some(config));
    let input = [
        framed(0, "slow", "enter"),
        framed(6 * SECOND, "slow", "return"),
    ]
    .join("\n");
    let output = run(&root, &input);
    assert_eq!(output.status.code(), Some(0));
    let records = records(&output);
    assert_eq!(records[0]["coverage"], json!(DEFAULTS));
    assert_eq!(records[0]["defaults"]["span.held"], "overridden");
}

#[test]
fn addition() {
    let config = r#"
version = 1
disable = ["span.prefix"]

[[analyzer]]
id = "cycle.held"
mapping = { kind = "held-time-nanoseconds", group = "agent.cycle" }
threshold = { above = 1 }
"#;
    let root = Root::new(Some(config));
    let output = run(&root, "");
    assert_eq!(output.status.code(), Some(0));
    let records = records(&output);
    assert_eq!(
        records[0]["coverage"],
        json!([
            "trace.size",
            "trace.prefix",
            "trace.held",
            "span.size",
            "span.held",
            "cycle.held"
        ])
    );
    assert_eq!(records[0]["defaults"]["span.prefix"], "disabled");
    assert!(records[0]["defaults"].get("cycle.held").is_none());
}

#[test]
fn remapped() {
    let config = r#"
version = 1

[[analyzer]]
id = "span.held"
mapping = { kind = "held-time-nanoseconds", group = "locus.trace" }
threshold = { above = 1 }
"#;
    assert!(refusal(config).contains("analyzer span.held must keep its default mapping"));
}

#[test]
fn contradiction() {
    let config = r#"
version = 1
disable = ["trace.size"]

[[analyzer]]
id = "trace.size"
mapping = { kind = "representation-bytes", group = "locus.trace" }
threshold = { above = 1 }
"#;
    assert!(refusal(config).contains("analyzer trace.size is both disabled and declared"));
}

#[test]
fn unknown() {
    let config = "version = 1\ndisable = [\"cycle.held\"]\n";
    assert!(refusal(config).contains("cannot disable unknown default analyzer: cycle.held"));
}

#[test]
fn repeated() {
    let config = "version = 1\ndisable = [\"span.held\", \"span.held\"]\n";
    assert!(refusal(config).contains("duplicate disabled analyzer: span.held"));
}

#[test]
fn absent() {
    let root = Root::new(None);
    let output = Command::new(env!("CARGO_BIN_EXE_locus"))
        .arg("inspect")
        .arg(root.path().join("absent"))
        .args(["--api", &stub::serve("")])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("not a directory")
    );
}
