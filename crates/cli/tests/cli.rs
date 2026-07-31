use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn atom(trace: Option<&str>, payload: Value) -> String {
    let mut context = serde_json::Map::new();
    if let Some(trace) = trace {
        context.insert("locus.trace".into(), Value::String(trace.into()));
    }
    json!({
        "at": 1,
        "context": context,
        "choices": [],
        "payload": payload,
    })
    .to_string()
}

fn run(input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_locus"))
        .arg("inspect")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn clean() {
    let input = ["alpha", "bravo", "charlie", "delta"]
        .map(|value| atom(Some("trace"), json!(value)))
        .join("\n");
    let output = run(&input);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn size() {
    let input = atom(Some("trace"), json!("x".repeat(9_000)));
    let output = run(&input);
    assert_eq!(output.status.code(), Some(1));
    let finding: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(finding["law"], "trace.size");
    assert_eq!(finding["bytes"], input.len());
    assert_eq!(finding["limit"], 8 * 1024);
}

#[test]
fn prefix() {
    let mut lines = (0..5)
        .map(|index| atom(Some("trace"), json!(format!("repeated-prefix-{index}"))))
        .collect::<Vec<_>>();
    lines.push(atom(Some("trace"), json!("different")));
    let output = run(&lines.join("\n"));
    assert_eq!(output.status.code(), Some(1));
    let finding: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(finding["law"], "trace.prefix");
    assert_eq!(finding["atoms"], 6);
    assert_eq!(finding["repeated"], 5);
    assert_eq!(finding["limit"], 80);
}

#[test]
fn density() {
    let mut lines = (0..4)
        .map(|index| atom(Some("trace"), json!(format!("repeated-prefix-{index}"))))
        .collect::<Vec<_>>();
    lines.push(atom(Some("trace"), json!("different")));
    let output = run(&lines.join("\n"));
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn envelope() {
    let input = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"]
        .map(|value| atom(Some("same-trace"), json!(value)))
        .join("\n");
    let output = run(&input);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn untraced() {
    let input = atom(None, json!("x".repeat(9_000)));
    let output = run(&input);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn malformed() {
    let output = run("{");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .starts_with("locus: invalid Atom on line 1:")
    );
}

#[test]
fn blank() {
    let input = format!("{}\n\n", atom(Some("trace"), json!("value")));
    let output = run(&input);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "locus: invalid Atom on line 2: empty record\n"
    );
}
