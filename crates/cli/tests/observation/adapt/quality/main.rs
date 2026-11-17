mod adapter;
mod command;
mod world;

use serde_json::{Value, json};
use std::fs;
use world::{atom, attempt, command, run};

#[test]
fn reconciliation() {
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("diagnostics.jsonl");
    let accepted = attempt("attempt", "one");
    fs::write(
        &log,
        format!("{accepted}\n{accepted}\nlocus adapter diagnostic [field.invalid]\n"),
    )
    .unwrap();
    let body = format!(
        "{}\n{}\n",
        atom("PreToolUse", "one"),
        atom("PostToolUse", "two")
    );
    let result = run(
        &body,
        command().args([
            "--diagnostics",
            log.to_str().unwrap(),
            "--expect-session",
            "session",
            "--expect-session",
            "missing",
            "--token",
            "private-token",
        ]),
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(!String::from_utf8_lossy(&result.stdout).contains("private-token"));
    assert_eq!(report["source"]["producer"], "locus-adapter-codex");
    assert_eq!(report["source"]["expected"], json!(["missing", "session"]));
    assert_eq!(report["schema"], "locus.quality/v1");
    assert_eq!(report["scope"]["expected"], 2);
    assert_eq!(report["scope"]["matched_expected"], 1);
    assert_eq!(report["tools"]["matched"], 1);
    assert_eq!(report["outcomes"]["observed"], 1);
    assert_eq!(report["outcomes"]["duplicate"], 1);
    assert_eq!(report["readback"]["matched"], 1);
    assert_eq!(report["readback"]["api_without_outcome_match"], 1);
    assert_eq!(report["legacy_unscoped"], 1);
    assert_eq!(report["retained"], 1);
}

#[test]
fn unknowns() {
    let result = run(
        &format!("{}\n", atom("SessionStart", "one")),
        &mut command(),
    );
    assert!(result.status.success());
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(report["outcomes"].is_null());
    assert!(report["readback"].is_null());
    assert!(report["scope"]["expected"].is_null());
    assert!(
        report["unknowns"]
            .as_array()
            .unwrap()
            .contains(&json!("active-population-denominator"))
    );
}

#[test]
fn exclusion() {
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("diagnostics.jsonl");
    fs::write(&log, format!("{}\n", attempt("attempt", "one"))).unwrap();
    let result = run(
        &format!("{}\n", atom("SessionStart", "one")),
        command().args([
            "--diagnostics",
            log.to_str().unwrap(),
            "--exclude-session",
            "session",
        ]),
    );
    assert!(result.status.success());
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["records"], 0);
    assert_eq!(report["outcomes"]["observed"], 0);
}

#[test]
fn refused() {
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("diagnostics.jsonl");
    for content in [
        "private-raw-input\n".into(),
        attempt("attempt", "one").to_string(),
        "{\"schema\":\"future\"}\n".into(),
    ] {
        fs::write(&log, content).unwrap();
        let output = command()
            .args([
                "--diagnostics",
                log.to_str().unwrap(),
                "--api",
                "http://127.0.0.1:1",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-raw-input"));
    }
}

#[test]
fn stages() {
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("diagnostics.jsonl");
    let mut pending = attempt("pending", "not-visible-yet");
    pending["outcome"]["buffer"] = json!("failed");
    let mut rejected = attempt("rejected", "unused");
    rejected["atom"] = Value::Null;
    rejected["outcome"] = json!({"status":"rejected","stage":"normalize"});
    rejected["issues"] = json!([{"stage":"normalize","code":"field.invalid","field":null}]);
    fs::write(&log, format!("{pending}\n{rejected}\n")).unwrap();
    let output = run("", command().args(["--diagnostics", log.to_str().unwrap()]));
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["outcomes"]["accepted"], 1);
    assert_eq!(report["outcomes"]["rejected"], 1);
    assert_eq!(report["outcomes"]["buffer_failed"], 1);
    assert_eq!(report["readback"]["accepted_without_api_match"], 1);
}

#[test]
fn corrupt() {
    let mut bad = atom("SessionStart", "one");
    bad["context"] = json!("private-raw-content");
    let output = run(&format!("{bad}\n"), &mut command());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private-raw-content"));
}
