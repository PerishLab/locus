#[path = "quality/main.rs"]
mod quality;

mod wire;

use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn invoke(buffer: &Path, input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_locus"))
        .args([
            "adapt",
            "codex",
            "--endpoint",
            "http://127.0.0.1:0",
            "--buffer",
        ])
        .arg(buffer)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let result = child.stdin.take().unwrap().write_all(input);
    let output = child.wait_with_output().unwrap();
    if input.len() <= 1024 * 1024 {
        result.unwrap();
    }
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"{}\n");
    output
}

fn records(buffer: &Path) -> Vec<Value> {
    std::fs::read_dir(buffer)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy();
            (name == "active.jsonl" || name.starts_with("sealed-"))
                .then(|| std::fs::read_to_string(path).unwrap())
        })
        .flat_map(|text| {
            text.lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn tool(event: &str, command: &str) -> Value {
    json!({"session_id":"session-1", "turn_id":"turn-1", "hook_event_name":event,
        "tool_use_id":"call-1", "tool_name":"Bash", "cwd":"private-cwd",
        "transcript_path":"private-transcript", "tool_input":{"command":command},
        "tool_response":{"output":"private-result", "process_id":123}})
}

#[test]
fn whitelist() {
    let root = tempfile::tempdir().unwrap();
    let buffer = root.path().join("buffer");
    for event in ["PreToolUse", "PostToolUse"] {
        invoke(
            &buffer,
            tool(
                event,
                "runseal :santi.soul.source santi strand send id 'private-prompt'",
            )
            .to_string()
            .as_bytes(),
        );
    }
    let records = records(&buffer);
    assert_eq!(records.len(), 2);
    assert_ne!(records[0]["id"], records[1]["id"]);
    for atom in records {
        assert_eq!(atom["producer"], "locus-adapter-codex");
        assert_eq!(atom["context"]["codex.session"], "session-1");
        assert_eq!(atom["context"]["codex.tool"], "call-1");
        assert_eq!(atom["payload"]["santi_operation"], "strand.send");
        assert_eq!(atom["payload"]["source_version"], "unavailable");
        assert!(atom["payload"].get("contract").is_none());
        assert!(atom.get("source").is_none());
        assert!(!atom.to_string().contains("private-"));
    }
}

#[test]
fn mentions() {
    let root = tempfile::tempdir().unwrap();
    for command in [
        "echo 'santi strand send'",
        "gh issue list --repo PerishLab/santi",
        "python3 script.py",
        "echo ok && santi health",
    ] {
        invoke(
            root.path(),
            tool("PreToolUse", command).to_string().as_bytes(),
        );
    }
    assert!(
        records(root.path())
            .iter()
            .all(|atom| atom["payload"]["tool_class"] == "unclassified")
    );
}

#[test]
fn failures() {
    let root = tempfile::tempdir().unwrap();
    let buffer = root.path().join("buffer");
    let cases = [
        b"private-invalid-json".to_vec(),
        vec![b'x'; 1024 * 1024 + 1],
        json!({"session_id":"session", "hook_event_name":"FutureEvent"})
            .to_string()
            .into_bytes(),
        json!({"session_id":"session", "hook_event_name":"PreToolUse"})
            .to_string()
            .into_bytes(),
    ];
    for input in cases {
        let output = invoke(&buffer, &input);
        assert!(!output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-"));
    }
    assert!(!buffer.exists());
    std::fs::write(&buffer, "occupied").unwrap();
    let output = invoke(
        &buffer,
        tool("PostToolUse", "santi health").to_string().as_bytes(),
    );
    assert!(!output.stderr.is_empty());
    assert_eq!(std::fs::read_to_string(&buffer).unwrap(), "occupied");
}

#[test]
fn arguments() {
    for args in [
        vec!["adapt", "codex"],
        vec!["adapt", "missing"],
        vec!["adapt", "codex", "--private-unknown"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_locus"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"{}\n");
        let outcome: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(outcome["schema"], "locus.adapter.attempt/v1");
        assert_eq!(outcome["issues"][0]["code"], "arguments.invalid");
        assert_eq!(outcome["outcome"]["status"], "rejected");
    }
}

#[test]
fn concurrent() {
    let root = tempfile::tempdir().unwrap();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let buffer = root.path().to_path_buf();
            std::thread::spawn(move || {
                invoke(
                    &buffer,
                    tool("PostToolUse", "santi health").to_string().as_bytes(),
                )
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(records(root.path()).len(), 8);
}

#[test]
fn closed() {
    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_locus"))
        .args(["adapt", "codex", "--buffer"])
        .arg(root.path())
        .args(["--endpoint", "http://127.0.0.1:0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child
        .stdin
        .take()
        .unwrap()
        .write_all(tool("PreToolUse", "santi health").to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    assert_eq!(records(root.path()).len(), 1);
}
