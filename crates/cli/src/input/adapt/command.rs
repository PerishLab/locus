use super::{codex::Event, tool};
use serde_json::{Value, json};

mod shape;

pub(super) fn project(input: &Value, hook: &Event) -> Option<Event> {
    if input.get("hook_event_name")?.as_str()? != "PreToolUse"
        || input.get("tool_name")?.as_str()? != "Bash"
    {
        return None;
    }
    let shape = read(input);
    let mut fields = serde_json::Map::new();
    for key in ["turn_id", "tool_use_id"] {
        fields.insert(key.into(), hook.fields.get(key)?.clone());
    }
    Some(Event {
        session: hook.session.clone(),
        fields,
        payload: Some(json!([
            shape.overrides,
            shape.wrapper,
            shape.program,
            shape.operation
        ])),
        roles: vec![
            ("codex.record", "command.v1"),
            (
                "codex.command",
                if shape.reason == "known" {
                    "known.v1"
                } else {
                    "unknown.v1"
                },
            ),
            ("codex.coverage", shape.reason),
            ("codex.wrapper", shape.wrapper),
        ],
    })
}

fn read(input: &Value) -> shape::Shape {
    let Some(command) = input.pointer("/tool_input/command").and_then(Value::as_str) else {
        return shape::Shape::opaque("command-unavailable");
    };
    if command.len() > 4096 {
        return shape::Shape::opaque("budget");
    }
    let words = match tool::words(command) {
        Ok(words) => words,
        Err(reason) => return shape::Shape::opaque(reason),
    };
    if words.len() > 32 || words.iter().any(|word| word.len() > 128) {
        return shape::Shape::opaque("budget");
    }
    shape::Shape::read(&words)
}
