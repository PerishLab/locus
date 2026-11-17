use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};

fn invoke(buffer: &Path, endpoint: &str, input: &Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_locus"))
        .args(["adapt", "codex", "--endpoint", endpoint, "--buffer"])
        .arg(buffer)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"{}\n");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private-"));
    serde_json::from_slice(&output.stderr).unwrap()
}

#[test]
fn offline() {
    let root = tempfile::tempdir().unwrap();
    let result = invoke(
        root.path(),
        "http://127.0.0.1:0",
        &json!({"session_id":"session", "hook_event_name":"SessionStart", "source":"startup", "prompt":"private-prompt"}),
    );
    assert_eq!(result["outcome"]["status"], "accepted");
    assert_eq!(result["outcome"]["buffer"], "written");
    assert!(result["atom"].is_string());
    assert_eq!(result["issues"][0]["code"], "reporter.registration");
}

#[test]
fn normalization() {
    let root = tempfile::tempdir().unwrap();
    let result = invoke(
        root.path(),
        "http://127.0.0.1:0",
        &json!({"session_id":"session", "hook_event_name":"SessionEnd", "prompt":"private-prompt"}),
    );
    assert_eq!(
        result["outcome"],
        json!({"status":"rejected","stage":"normalize"})
    );
    assert_eq!(result["issues"][0]["code"], "field.invalid");
    assert_eq!(result["issues"][0]["field"], "reason");
    assert!(result["atom"].is_null());
}

#[test]
fn bootstrap() {
    let root = tempfile::tempdir().unwrap();
    let buffer = root.path().join("private-buffer");
    std::fs::write(&buffer, "occupied").unwrap();
    let result = invoke(
        &buffer,
        "http://127.0.0.1:0",
        &json!({"session_id":"session", "hook_event_name":"SessionStart", "source":"startup"}),
    );
    assert_eq!(
        result["outcome"],
        json!({"status":"rejected","stage":"bootstrap"})
    );
    assert!(result["atom"].is_null());
}

#[test]
fn reporting() {
    let root = tempfile::tempdir().unwrap();
    let buffer = root.path().join("buffer");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let active = buffer.join("active.jsonl");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().unwrap();
            }
        }
        reader.read_exact(&mut vec![0; length]).unwrap();
        std::fs::create_dir(active).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
    });
    let result = invoke(
        &buffer,
        &endpoint,
        &json!({"session_id":"session", "hook_event_name":"SessionStart", "source":"startup"}),
    );
    server.join().unwrap();
    assert_eq!(
        result["outcome"],
        json!({"status":"accepted","buffer":"failed"})
    );
    assert!(result["atom"].is_string());
    assert_eq!(result["issues"][0]["code"], "reporter.failed");
}
