use serde_json::{Map, Value, json};

pub(super) struct Event {
    pub session: String,
    pub fields: Map<String, Value>,
}

pub(super) fn normalize(bytes: &[u8]) -> Result<Event, &'static str> {
    let input: Value = serde_json::from_slice(bytes).map_err(|_| "input.json")?;
    let session = text(&input, "session_id")?.to_string();
    let name = text(&input, "hook_event_name")?;
    let mut fields = Map::new();
    fields.insert("schema".into(), json!("locus.adapter.codex/v1"));
    fields.insert("supported_contract".into(), json!("codex.hooks/0.169"));
    fields.insert("source_version".into(), json!("unavailable"));
    fields.insert("event".into(), json!(name));
    fields.insert("source_identity".into(), json!("correlation-only"));
    match name {
        "SessionStart" => copy(&input, &mut fields, "source")?,
        "SessionEnd" => {
            copy(&input, &mut fields, "reason")?;
            copy(&input, &mut fields, "turn_id")?;
        }
        "PreToolUse" | "PostToolUse" | "PermissionRequest" => {
            copy(&input, &mut fields, "turn_id")?;
            copy(&input, &mut fields, "tool_use_id")?;
            copy(&input, &mut fields, "tool_name")?;
            super::tool::observe(&input, &mut fields);
        }
        "SubagentStart" | "SubagentStop" => {
            copy(&input, &mut fields, "turn_id")?;
            copy(&input, &mut fields, "agent_id")?;
            copy(&input, &mut fields, "agent_type")?;
        }
        "UserPromptSubmit" | "Stop" | "Interrupt" => {
            copy(&input, &mut fields, "turn_id")?;
        }
        "PreCompact" | "PostCompact" => {
            copy(&input, &mut fields, "trigger")?;
            copy(&input, &mut fields, "turn_id")?;
        }
        "PreModelRequest" | "PostModelRequest" => {
            return Err("event.unsupported");
        }
        _ => return Err("event.unknown"),
    }
    optional(&input, &mut fields, "model")?;
    optional(&input, &mut fields, "agent_id")?;
    if let Some(value) = input.get("stop_hook_active") {
        if !value.is_boolean() {
            return Err("field.invalid");
        }
        fields.insert("stop_hook_active".into(), value.clone());
    }
    Ok(Event { session, fields })
}

fn text<'a>(input: &'a Value, key: &str) -> Result<&'a str, &'static str> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty() && text.len() <= 512)
        .filter(|text| !text.chars().any(char::is_control))
        .ok_or("field.invalid")
}

fn copy(input: &Value, fields: &mut Map<String, Value>, key: &str) -> Result<(), &'static str> {
    fields.insert(key.into(), json!(text(input, key)?));
    Ok(())
}

fn optional(input: &Value, fields: &mut Map<String, Value>, key: &str) -> Result<(), &'static str> {
    if input.get(key).is_some_and(|value| !value.is_null()) {
        copy(input, fields, key)?;
    }
    Ok(())
}
