#[path = "inspect/fixture.rs"]
mod fixture;

use fixture::{CONFIG, Root, atom, records, run};
use serde_json::{Value, json};

#[test]
fn clean() {
    let root = Root::new(Some(CONFIG));
    let input = ["alpha", "bravo", "charlie", "delta"]
        .map(|value| atom(Some("trace"), json!(value)))
        .join("\n");
    let output = run(&root, &input);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let records = records(&output);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["kind"], "summary");
    assert_eq!(
        records[0]["coverage"],
        json!(["trace.size", "trace.prefix"])
    );
    assert_eq!(records[0]["records"], 4);
    assert_eq!(records[0]["findings"], 0);
}

#[test]
fn size() {
    let root = Root::new(Some(CONFIG));
    let input = atom(Some("trace"), json!("x".repeat(9_000)));
    let output = run(&root, &input);
    assert_eq!(output.status.code(), Some(1));
    let records = records(&output);
    assert_eq!(records[0]["kind"], "finding");
    assert_eq!(records[0]["analyzer"], "trace.size");
    assert_eq!(records[0]["group"]["role"], "locus.trace");
    assert_eq!(records[0]["group"]["key"], "trace");
    assert_eq!(records[0]["measurement"]["mapping"], "representation-bytes");
    assert_eq!(records[0]["measurement"]["value"], input.len());
    assert_eq!(records[0]["threshold"]["above"], 8 * 1024);
    assert_eq!(records[1]["findings"], 1);
}

#[test]
fn prefix() {
    let root = Root::new(Some(CONFIG));
    let mut lines = (0..5)
        .map(|index| atom(Some("trace"), json!(format!("repeated-prefix-{index}"))))
        .collect::<Vec<_>>();
    lines.push(atom(Some("trace"), json!("different")));
    let output = run(&root, &lines.join("\n"));
    assert_eq!(output.status.code(), Some(1));
    let records = records(&output);
    assert_eq!(records[0]["analyzer"], "trace.prefix");
    assert_eq!(
        records[0]["measurement"]["mapping"],
        "dominant-content-prefix-percent"
    );
    assert_eq!(records[0]["measurement"]["atoms"], 6);
    assert_eq!(records[0]["measurement"]["repeated"], 5);
    assert_eq!(records[0]["threshold"]["above"], 80);
}

#[test]
fn density() {
    let root = Root::new(Some(CONFIG));
    let mut lines = (0..4)
        .map(|index| atom(Some("trace"), json!(format!("repeated-prefix-{index}"))))
        .collect::<Vec<_>>();
    lines.push(atom(Some("trace"), json!("different")));
    let output = run(&root, &lines.join("\n"));
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(records(&output)[0]["findings"], 0);
}

#[test]
fn envelope() {
    let root = Root::new(Some(CONFIG));
    let input = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"]
        .map(|value| atom(Some("same-trace"), json!(value)))
        .join("\n");
    let output = run(&root, &input);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(records(&output)[0]["findings"], 0);
}

#[test]
fn untraced() {
    let root = Root::new(Some(CONFIG));
    let input = atom(None, json!("x".repeat(9_000)));
    let output = run(&root, &input);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(records(&output)[0]["findings"], 0);
}

#[test]
fn coverage() {
    let root = Root::new(Some("version = 1\n"));
    let output = run(&root, &atom(Some("trace"), json!("value")));
    assert_eq!(output.status.code(), Some(0));
    let records = records(&output);
    assert_eq!(records[0]["coverage"], json!([]));
    assert_eq!(records[0]["records"], 1);
}

#[test]
fn threshold() {
    let config = CONFIG.replace("above = 8192", "above = 10000");
    let root = Root::new(Some(&config));
    let output = run(&root, &atom(Some("trace"), json!("x".repeat(9_000))));
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(records(&output)[0]["findings"], 0);
}

#[test]
fn role() {
    let config = CONFIG.replace("locus.trace", "agent.cycle");
    let root = Root::new(Some(&config));
    let mut input: Value = serde_json::from_str(&atom(None, json!("x".repeat(9_000)))).unwrap();
    input["context"]["agent.cycle"] = json!("cycle");
    let output = run(&root, &input.to_string());
    assert_eq!(output.status.code(), Some(1));
    let records = records(&output);
    assert_eq!(records[0]["group"]["role"], "agent.cycle");
    assert_eq!(records[0]["group"]["key"], "cycle");
}

#[test]
fn missing() {
    let root = Root::new(None);
    let output = run(&root, "");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .starts_with("locus: cannot read ")
    );
}

#[test]
fn invalid() {
    let root = Root::new(Some("version = 1\nextra = true\n"));
    let output = run(&root, "");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unknown field `extra`")
    );
}

#[test]
fn unknown() {
    let config = CONFIG.replace("representation-bytes", "mystery");
    let root = Root::new(Some(&config));
    let output = run(&root, "");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unknown variant `mystery`")
    );
}

#[test]
fn malformed() {
    let root = Root::new(Some(CONFIG));
    let output = run(&root, "{");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .starts_with("locus: invalid Atom on line 1:")
    );
}

#[test]
fn blank() {
    let root = Root::new(Some(CONFIG));
    let input = format!("{}\n\n", atom(Some("trace"), json!("value")));
    let output = run(&root, &input);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "locus: invalid Atom on line 2: empty record\n"
    );
}
