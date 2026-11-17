use super::{atoms, invoke, tool};
use serde_json::{Value, json};

fn shape(command: &str) -> Value {
    let root = tempfile::tempdir().unwrap();
    let output = invoke(
        root.path(),
        tool("PreToolUse", command).to_string().as_bytes(),
    );
    let diagnostic: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(diagnostic["schema"], "locus.adapter.attempt/v2");
    let records = atoms(root.path());
    assert_eq!(records.len(), 2);
    assert_eq!(diagnostic["hook"]["atom"], records[0]["id"]);
    assert_eq!(diagnostic["command"]["atom"], records[1]["id"]);
    assert_ne!(diagnostic["hook"]["id"], diagnostic["command"]["id"]);
    let atom = records[1].clone();
    assert!(atom.get("source").is_none());
    assert_eq!(atom["context"]["codex.session"], "session-1");
    assert_eq!(atom["context"]["codex.turn"], "turn-1");
    assert_eq!(atom["context"]["codex.tool"], "call-1");
    assert_eq!(atom["context"]["codex.record"], "command.v1");
    assert!(atom["context"].get("locus.trace").is_none());
    assert!(atom.to_string().len() < 4096);
    atom
}

#[test]
fn projection() {
    for (command, expected) in [
        (
            "gh issue list --repo private-repo",
            json!([[], "none", "gh", "issue"]),
        ),
        (
            "runseal :private-seat gh api user",
            json!([[], "runseal", "gh", "api"]),
        ),
        (
            "env GODEBUG=http2client=0 gh api user",
            json!([["GODEBUG=http2client=0"], "env", "gh", "api"]),
        ),
        (
            "runseal :private-seat env GODEBUG=http2client=0 concord issue show x",
            json!([["GODEBUG=http2client=0"], "runseal-env", "concord", "issue"]),
        ),
        (
            "santi strand send id 'private-prompt'",
            json!([[], "none", "santi", "strand.send"]),
        ),
        (
            "plumb release --help",
            json!([[], "none", "plumb", "release"]),
        ),
    ] {
        let atom = shape(command);
        assert_eq!(atom["payload"], expected);
        assert_eq!(atom["context"]["codex.command"], "known.v1");
        assert!(!atom.to_string().contains("private-"));
    }
}

#[test]
fn unknown() {
    for (command, reason) in [
        ("gh api user; santi health", "shell-structure"),
        ("gh api $(private-command)", "shell-expansion"),
        ("gh api `private-command`", "shell-expansion"),
        ("gh api user > private-file", "shell-structure"),
        ("gh api user\nsanti health", "shell-structure"),
        ("/private/gh api user", "program-unresolved"),
        ("env -i gh api user", "program-unresolved"),
        ("env env gh api user", "program-unresolved"),
        (
            "gh --hostname private-host api user",
            "operation-unresolved",
        ),
        ("python3 private-script", "program-unresolved"),
    ] {
        let atom = shape(command);
        assert_eq!(atom["context"]["codex.command"], "unknown.v1");
        assert_eq!(atom["context"]["codex.coverage"], reason);
        assert_eq!(atom["payload"], json!([[], "opaque", "other", "other"]));
        assert!(!atom.to_string().contains("private"));
    }
}

#[test]
fn privacy() {
    let atom = shape(
        "runseal :private-seat env GH_TOKEN=private-token GODEBUG=private-debug gh api /private/path --field password=private-value",
    );
    assert_eq!(
        atom["payload"],
        json!([["other", "other"], "runseal-env", "gh", "api"])
    );
    for sentinel in ["private", "GH_TOKEN", "password", "GODEBUG", "/path"] {
        assert!(!atom.to_string().contains(sentinel));
    }
    let quoted = shape("gh api 'private;$HOME|$(command)' --field key=private-value");
    assert_eq!(quoted["context"]["codex.command"], "known.v1");
    assert!(!quoted.to_string().contains("private"));
}

#[test]
fn bounds() {
    for command in [
        format!("gh api {}", "x".repeat(129)),
        format!("gh api {}", "x ".repeat(31)),
        "x".repeat(4097),
    ] {
        assert_eq!(shape(&command)["context"]["codex.coverage"], "budget");
    }
    assert_eq!(
        shape(&format!("gh api {}", "x".repeat(128)))["context"]["codex.command"],
        "known.v1"
    );
    let root = tempfile::tempdir().unwrap();
    let mut input = tool("PreToolUse", "ignored");
    input["tool_input"] = json!({});
    invoke(root.path(), input.to_string().as_bytes());
    assert_eq!(
        atoms(root.path())[1]["context"]["codex.coverage"],
        "command-unavailable"
    );
}

#[test]
fn scope() {
    for event in ["PostToolUse", "PermissionRequest"] {
        let root = tempfile::tempdir().unwrap();
        invoke(
            root.path(),
            tool(event, "gh api user").to_string().as_bytes(),
        );
        assert_eq!(atoms(root.path()).len(), 1);
    }
    let root = tempfile::tempdir().unwrap();
    let mut input = tool("PreToolUse", "gh api user");
    input["tool_name"] = json!("mcp_private");
    invoke(root.path(), input.to_string().as_bytes());
    assert_eq!(atoms(root.path()).len(), 1);
}

#[test]
fn failure() {
    let root = tempfile::tempdir().unwrap();
    let buffer = root.path().join("occupied");
    std::fs::write(&buffer, "private-data").unwrap();
    let output = invoke(
        &buffer,
        tool("PreToolUse", "gh api user").to_string().as_bytes(),
    );
    let record: Value = serde_json::from_slice(&output.stderr).unwrap();
    for part in ["hook", "command"] {
        assert_eq!(record[part]["outcome"]["status"], "rejected");
        assert!(record[part]["atom"].is_null());
    }
    assert_eq!(std::fs::read_to_string(buffer).unwrap(), "private-data");
}

#[test]
fn maximum() {
    let root = tempfile::tempdir().unwrap();
    let command = format!("env {}gh api", "GODEBUG=http2client=0 ".repeat(29));
    let mut input = tool("PreToolUse", &command);
    for key in ["session_id", "turn_id", "tool_use_id"] {
        input[key] = json!("x".repeat(512));
    }
    let output = invoke(root.path(), input.to_string().as_bytes());
    assert!(output.stderr.len() < 16384);
    let records = atoms(root.path());
    assert_eq!(records.len(), 2);
    assert!(records[1].to_string().len() < 8192);
    assert_eq!(records[1]["payload"][0].as_array().unwrap().len(), 29);
}
