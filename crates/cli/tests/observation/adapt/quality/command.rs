use super::world::{atom, attempt, command, run};
use serde_json::{Value, json};

fn projected() -> Value {
    json!({"at":100,"id":"shape-atom","producer":"locus-adapter-codex","choices":[],
        "context":{"codex.session":"session","codex.turn":"turn","codex.tool":"tool","codex.record":"command.v1","codex.command":"known.v1","codex.coverage":"known"},
        "payload":[["GODEBUG=http2client=0"],"env","gh","api"]})
}

fn paired() -> Value {
    let mut shape = attempt("shape-attempt", "shape-atom");
    shape["event"] = json!("CommandShape");
    json!({"schema":"locus.adapter.attempt/v2","hook":attempt("hook-attempt","hook-atom"),"command":shape})
}

#[test]
fn reconciliation() {
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("log");
    let pair = paired();
    let old = attempt("legacy", "legacy-atom");
    std::fs::write(&log, format!("{old}\n{pair}\n{pair}\n")).unwrap();
    let input = format!("{}\n{}\n", atom("PreToolUse", "hook-atom"), projected());
    let output = run(
        &input,
        command().args(["--diagnostics", log.to_str().unwrap()]),
    );
    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["records"], 1);
    assert_eq!(report["tools"]["before"], 1);
    assert_eq!(report["outcomes"]["observed"], 2);
    assert_eq!(report["outcomes"]["duplicate"], 1);
    assert_eq!(report["readback"]["matched"], 1);
    assert_eq!(report["readback"]["accepted_without_api_match"], 1);
    let shapes = &report["commands"];
    assert_eq!(shapes["records"], 1);
    assert_eq!(shapes["tools"]["before"], 0);
    assert_eq!(shapes["outcomes"]["observed"], 1);
    assert_eq!(shapes["outcomes"]["duplicate"], 1);
    assert_eq!(shapes["readback"]["matched"], 1);
    assert_eq!(shapes["classes"]["known"], 1);
}

#[test]
fn partial() {
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("log");
    std::fs::write(&log, format!("{}\n", paired())).unwrap();
    let output = run(
        &format!("{}\n", atom("PreToolUse", "hook-atom")),
        command().args(["--diagnostics", log.to_str().unwrap()]),
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["readback"]["accepted_without_api_match"], 0);
    assert_eq!(
        report["commands"]["readback"]["accepted_without_api_match"],
        1
    );
    let output = run(
        &format!("{}\n", projected()),
        command().args([
            "--diagnostics",
            log.to_str().unwrap(),
            "--exclude-session",
            "session",
        ]),
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["outcomes"]["observed"], 0);
    assert_eq!(report["commands"]["records"], 0);
    assert_eq!(report["commands"]["outcomes"]["observed"], 0);
}

#[test]
fn malformed() {
    for payload in [
        json!("private-data"),
        json!([[], "env"]),
        json!([3, "env", "gh", "api"]),
    ] {
        let mut bad = projected();
        bad["payload"] = payload;
        let output = run(&format!("{bad}\n"), &mut command());
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-data"));
    }
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("log");
    let mut bad = paired();
    bad["command"]["group"] = json!("different-session");
    std::fs::write(&log, format!("{bad}\n")).unwrap();
    let output = command()
        .args([
            "--api",
            "http://127.0.0.1:0",
            "--diagnostics",
            log.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn identity() {
    let mut shape = projected();
    shape["id"] = json!("shared");
    let input = format!("{}\n{shape}\n", atom("PreToolUse", "shared"));
    let output = run(&input, &mut command());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("log");
    let mut pair = paired();
    pair["command"]["atom"] = pair["hook"]["atom"].clone();
    std::fs::write(&log, format!("{pair}\n")).unwrap();
    let output = command()
        .args([
            "--api",
            "http://127.0.0.1:0",
            "--diagnostics",
            log.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
