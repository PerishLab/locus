use super::outcome::Fault;
use serde_json::{Map, Value, json};

mod wire;

pub(super) struct Event {
    pub session: String,
    pub fields: Map<String, Value>,
}

pub(super) fn normalize(bytes: &[u8]) -> Result<Event, Fault> {
    let input: Value = serde_json::from_slice(bytes).map_err(|_| Fault::from("input.json"))?;
    let name = text(&input, "hook_event_name")?;
    if matches!(name, "PreModelRequest" | "PostModelRequest") {
        return Err("event.unsupported".into());
    }
    let envelope: wire::Envelope = serde_json::from_value(input.clone()).map_err(Fault::wire)?;
    if matches!(envelope.event, wire::Event::Unknown) {
        return Err("event.unknown".into());
    }
    let shell = envelope.event.shell();
    let mut fields = envelope.event.fields().map_err(Fault::from)?;
    fields.insert("schema".into(), json!("locus.adapter.codex/v1"));
    fields.insert("supported_contract".into(), json!("codex.hooks/0.169"));
    fields.insert("source_version".into(), json!("unavailable"));
    fields.insert("source_identity".into(), json!("correlation-only"));
    if shell {
        fields.insert(
            "classification_profile".into(),
            json!("santi.command/simple-v1"),
        );
        super::tool::observe(&input, &mut fields);
    }
    optional(&input, &mut fields, "model")?;
    optional(&input, &mut fields, "agent_id")?;
    if let Some(value) = input.get("stop_hook_active") {
        if !value.is_boolean() {
            return Err(Fault::field("stop_hook_active"));
        }
        fields.insert("stop_hook_active".into(), value.clone());
    }
    Ok(Event {
        session: envelope.session.0,
        fields,
    })
}

fn text<'a>(input: &'a Value, key: &'static str) -> Result<&'a str, Fault> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty() && text.len() <= 512)
        .filter(|text| !text.chars().any(char::is_control))
        .ok_or_else(|| Fault::field(key))
}

fn copy(input: &Value, fields: &mut Map<String, Value>, key: &'static str) -> Result<(), Fault> {
    fields.insert(key.into(), json!(text(input, key)?));
    Ok(())
}

fn optional(
    input: &Value,
    fields: &mut Map<String, Value>,
    key: &'static str,
) -> Result<(), Fault> {
    if input.get(key).is_some_and(|value| !value.is_null()) {
        copy(input, fields, key)?;
    }
    Ok(())
}
