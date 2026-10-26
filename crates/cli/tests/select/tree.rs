use super::stub;

use serde_json::{Value, json};
use std::process::Output;

fn atom(at: u64, context: Value, payload: Value) -> String {
    json!({"at": at, "context": context, "choices": [], "payload": payload}).to_string()
}

fn input() -> String {
    [
        atom(
            1,
            json!({"locus.trace": "root", "locus.span": "r"}),
            json!({"event": "cli.start"}),
        ),
        atom(
            2,
            json!({"locus.trace": "work", "locus.span": "w", "locus.parent": "r"}),
            json!({"outcome": "refused"}),
        ),
        atom(
            3,
            json!({"locus.trace": "root", "locus.span": "r"}),
            json!({"event": "cli.finish", "code": 0}),
        ),
        atom(
            4,
            json!({"locus.trace": "orphan", "locus.span": "o", "locus.parent": "gone"}),
            json!({"outcome": "refused"}),
        ),
    ]
    .join("\n")
}

fn run(args: &[&str]) -> Output {
    stub::locus()
        .arg("tree")
        .args(args)
        .args(["--api", &stub::serve(&input())])
        .output()
        .expect("output")
}

fn records(output: &Output) -> Vec<Value> {
    String::from_utf8(output.stdout.clone())
        .expect("utf8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("record"))
        .collect()
}

#[test]
fn expanded() {
    let output = run(&[
        "--seed",
        "payload.outcome=refused",
        "--show",
        "payload.event=cli.finish",
    ]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let records = records(&output);
    let kinds: Vec<&Value> = records.iter().map(|record| &record["kind"]).collect();
    assert_eq!(kinds, ["tree", "atom", "atom", "tree", "atom", "summary"]);
    assert!(
        records
            .iter()
            .all(|record| record["schema"] == "locus.tree/v1")
    );
    assert_eq!(
        (records[0]["root"].clone(), records[0]["traces"].clone()),
        (json!("root"), json!(2))
    );
    assert_eq!(records[2]["atom"]["payload"]["code"], 0);
    assert_eq!(
        (records[3]["root"].clone(), records[3]["detached"].clone()),
        (json!("orphan"), json!(true))
    );
    let summary = &records[5];
    assert_eq!(summary["seed"], json!(["payload.outcome=refused"]));
    assert_eq!(summary["show"], json!(["payload.event=cli.finish"]));
    assert_eq!(summary["bytes"], input().len());
    assert_eq!(
        (summary["records"].clone(), summary["trees"].clone()),
        (json!(4), json!(2))
    );
    assert_eq!(
        summary["limits"],
        json!({"trees": 1000, "atoms": 100000, "keys": 2000000})
    );
    assert_eq!(summary["links"]["missing"], 1);
    assert!(summary["version"].is_string());
    assert!(summary["request"]["api"].is_string());
}

#[test]
fn incomplete() {
    let output = run(&[
        "--seed",
        "payload.outcome=refused",
        "--from",
        "1",
        "--to",
        "1000",
    ]);
    let records = records(&output);
    let summary = records.last().expect("summary");
    assert_eq!(summary["incomplete"], json!(["window.from", "window.to"]));
}

#[test]
fn refused() {
    let cases: [&[&str]; 4] = [
        &["--seed", "payload.outcome=refused", "--max-trees", "1"],
        &["--seed", "payload.outcome=refused", "--max-atoms", "1"],
        &["--seed", "payload.outcome=refused", "--max-keys", "3"],
        &["--seed", "outcome=refused"],
    ];
    for args in cases {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).starts_with("locus: "),
            "{args:?}"
        );
    }
    let output = stub::locus()
        .args(["tree", "--api", "http://127.0.0.1:9"])
        .output()
        .expect("output");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
