use super::{invoke, records, tool};
use serde_json::json;

#[test]
fn lifecycle() {
    let root = tempfile::tempdir().unwrap();
    for event in [
        json!({"hook_event_name":"SessionStart", "source":"resume"}),
        json!({"hook_event_name":"UserPromptSubmit", "prompt":"private-user"}),
        json!({"hook_event_name":"PreCompact", "trigger":"auto"}),
        json!({"hook_event_name":"PostCompact", "trigger":"auto"}),
        json!({"hook_event_name":"SubagentStart", "agent_id":"child", "agent_type":"worker"}),
        json!({"hook_event_name":"SubagentStop", "agent_id":"child", "agent_type":"worker", "stop_hook_active":true}),
        json!({"hook_event_name":"Stop", "stop_hook_active":false, "last_assistant_message":"private-reply"}),
        json!({"hook_event_name":"Interrupt"}),
        json!({"hook_event_name":"SessionEnd", "reason":"other"}),
    ] {
        let mut event = event;
        event["session_id"] = json!("parent");
        event["turn_id"] = json!("turn");
        invoke(root.path(), event.to_string().as_bytes());
    }
    let records = records(root.path());
    assert_eq!(records.len(), 9);
    assert_eq!(
        records
            .iter()
            .filter(|atom| atom["context"]["codex.agent"] == "child")
            .count(),
        2
    );
    assert!(
        records
            .iter()
            .all(|atom| !atom.to_string().contains("private-"))
    );
}

#[test]
fn partial() {
    let root = tempfile::tempdir().unwrap();
    let before = tool("PreToolUse", "santi health");
    invoke(root.path(), before.to_string().as_bytes());
    invoke(root.path(), before.to_string().as_bytes());
    let mut after = tool("PostToolUse", "santi health");
    after["agent_id"] = json!("child");
    after["tool_response"] = json!({"exit_code":1,"output":"private-error"});
    invoke(root.path(), after.to_string().as_bytes());
    let atoms = records(root.path());
    assert_eq!(atoms.len(), 3);
    assert_ne!(atoms[0]["id"], atoms[1]["id"]);
    assert_eq!(atoms[2]["context"]["codex.agent"], "child");
    assert!(
        atoms
            .iter()
            .all(|atom| atom["payload"].get("success").is_none())
    );
}

#[test]
fn references() {
    let root = tempfile::tempdir().unwrap();
    let mut event = tool("PostToolUse", "santi strand send strand 'private-task'");
    let inbox = "inbox_0123456789abcdef0123456789abcdef";
    event["tool_response"] = json!({"output":format!("private-result {inbox} {inbox}")});
    invoke(root.path(), event.to_string().as_bytes());
    let atoms = records(root.path());
    assert_eq!(atoms[0]["payload"]["santi_references"], json!([inbox]));
    assert_eq!(
        atoms[0]["payload"]["reference_basis"],
        "tool-output-candidates"
    );
    assert!(!atoms[0].to_string().contains("private-"));
}

#[test]
fn protocol() {
    let root = tempfile::tempdir().unwrap();
    let mut event = tool("PreToolUse", "santi health");
    event["tool_name"] = json!("mcp__custom__command");
    invoke(root.path(), event.to_string().as_bytes());
    event["tool_name"] = json!("Bash");
    event["tool_input"] = json!({"command":"santi strand send id -- -h"});
    invoke(root.path(), event.to_string().as_bytes());
    let atoms = records(root.path());
    assert_eq!(atoms[0]["payload"]["classification_reason"], "not-shell");
    assert!(atoms[0]["payload"].get("santi_operation").is_none());
    assert_eq!(atoms[1]["payload"]["santi_operation"], "strand.send");
}
