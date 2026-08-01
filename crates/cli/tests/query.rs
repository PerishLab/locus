use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn atom(trace: Option<&str>, span: Option<&str>, payload: &str) -> String {
    let mut context = serde_json::Map::new();
    if let Some(trace) = trace {
        context.insert("locus.trace".into(), Value::String(trace.into()));
    }
    if let Some(span) = span {
        context.insert("locus.span".into(), Value::String(span.into()));
    }
    json!({
        "at": 1,
        "context": context,
        "choices": [],
        "payload": payload,
    })
    .to_string()
}

fn run(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_locus"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write");
    child.wait_with_output().expect("output")
}

fn records(output: &Output) -> Vec<Value> {
    String::from_utf8(output.stdout.clone())
        .expect("utf8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("record"))
        .collect()
}

#[test]
fn identities() {
    let input = [
        atom(Some("bravo"), None, "b"),
        atom(None, None, "unbound"),
        atom(Some("alpha"), Some("one"), "a1"),
        atom(Some("alpha"), Some("two"), "a2"),
    ]
    .join("\n");
    let output = run(&["query", "locus.trace"], &input);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let records = records(&output);
    assert_eq!(records.len(), 3);
    assert_eq!(records[0]["kind"], "identity");
    assert_eq!(records[0]["key"], "alpha");
    assert_eq!(records[0]["records"], 2);
    assert_eq!(records[1]["key"], "bravo");
    assert_eq!(records[2]["kind"], "summary");
    assert_eq!(records[2]["records"], 4);
    assert_eq!(records[2]["matched"], 3);
    assert_eq!(records[2]["identities"], 2);
}

#[test]
fn replay() {
    let input = [
        atom(Some("alpha"), Some("one"), "first"),
        atom(Some("bravo"), Some("two"), "other"),
        atom(Some("alpha"), Some("three"), "second"),
    ]
    .join("\n");
    let output = run(&["query", "locus.trace", "alpha"], &input);

    assert!(output.status.success());
    let records = records(&output);
    assert_eq!(records.len(), 3);
    assert_eq!(records[0]["kind"], "atom");
    assert_eq!(records[0]["atom"]["payload"], "first");
    assert_eq!(records[1]["atom"]["payload"], "second");
    assert_eq!(records[2]["records"], 3);
    assert_eq!(records[2]["matched"], 2);
    assert_eq!(records[2]["identities"], 1);
}

#[test]
fn empty() {
    let output = run(&["query", "locus.span", "missing"], "");

    assert!(output.status.success());
    let records = records(&output);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["kind"], "summary");
    assert_eq!(records[0]["key"], "missing");
    assert_eq!(records[0]["records"], 0);
    assert_eq!(records[0]["matched"], 0);
    assert_eq!(records[0]["identities"], 0);
}

#[test]
fn malformed() {
    let input = format!("{}\nnot-json\n", atom(Some("alpha"), None, "first"));
    let output = run(&["query", "locus.trace"], &input);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr")
            .contains("invalid Atom on line 2")
    );
}

#[test]
fn role() {
    let output = run(&["query", "bad role"], "");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .expect("stderr")
            .contains("invalid role")
    );
}
