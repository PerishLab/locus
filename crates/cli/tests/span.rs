use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn atom(at: u64, span: &str, edge: &str, function: &str) -> String {
    json!({
        "at": at,
        "context": {"locus.trace": "t", "locus.span": span},
        "choices": [],
        "source": {
            "file": "a.rs",
            "line": 1,
            "column": 1,
            "module": "m",
            "function": function,
            "edge": edge,
        },
    })
    .to_string()
}

fn run(input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_locus"))
        .arg("span")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write");
    child.wait_with_output().expect("output")
}

fn records(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("record"))
        .collect()
}

#[test]
fn held() {
    let input = [
        atom(0, "a", "enter", "outer"),
        atom(10, "b", "enter", "inner"),
        atom(20, "b", "return", "inner"),
        atom(30, "a", "return", "outer"),
    ]
    .join("\n");
    let output = run(&input);
    assert!(output.status.success());
    let records = records(&output);
    let outer = records
        .iter()
        .find(|record| record["declaration"] == "m::outer")
        .expect("outer");
    assert_eq!(outer["inclusive"], 30);
    assert_eq!(outer["held"], 20);
    let inner = records
        .iter()
        .find(|record| record["declaration"] == "m::inner")
        .expect("inner");
    assert_eq!(inner["inclusive"], 10);
    assert_eq!(inner["held"], 10);
    let summary = records.last().expect("summary");
    assert_eq!(summary["kind"], "summary");
    assert_eq!(summary["unclosed"], 0);
    assert_eq!(summary["matched"], 4);
}

#[test]
fn overlap() {
    let input = [
        atom(0, "a", "enter", "outer"),
        atom(10, "b", "enter", "inner"),
        atom(20, "a", "return", "outer"),
        atom(30, "b", "return", "inner"),
    ]
    .join("\n");
    let output = run(&input);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("overlap without nesting"), "{message}");
    assert!(message.contains("underivable"), "{message}");
}

#[test]
fn unclosed() {
    let output = run(&atom(0, "a", "enter", "died"));
    assert!(output.status.success());
    let records = records(&output);
    let unclosed = records.first().expect("unclosed");
    assert_eq!(unclosed["kind"], "unclosed");
    assert_eq!(unclosed["declaration"], "m::died");
    assert_eq!(unclosed["span"], "a");
    assert_eq!(unclosed["trace"], "t");
    let summary = records.last().expect("summary");
    assert_eq!(summary["unclosed"], 1);
    assert_eq!(summary["declarations"], 0);
}

#[test]
fn stray() {
    let output = run(&atom(0, "a", "return", "ghost"));
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("returns without entering"), "{message}");
}
