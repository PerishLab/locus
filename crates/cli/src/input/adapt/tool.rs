use serde_json::{Map, Value, json};
use std::path::Path;

mod shell;
pub(super) use shell::words;
#[cfg(test)]
mod tests;

pub(super) fn observe(input: &Value, fields: &mut Map<String, Value>) {
    if input.get("tool_name").and_then(Value::as_str) != Some("Bash") {
        fields.insert("classification_reason".into(), json!("not-shell"));
        return;
    }
    let command = input.pointer("/tool_input/command").and_then(Value::as_str);
    let words = command.map(shell::words).transpose();
    let operation = words
        .as_ref()
        .ok()
        .and_then(Option::as_deref)
        .and_then(operation);
    fields.insert(
        "tool_class".into(),
        json!(if operation.is_some() {
            "santi"
        } else {
            "unclassified"
        }),
    );
    fields.insert(
        "classification_reason".into(),
        json!(if operation.is_some() {
            "executable-and-operation"
        } else if command.is_none() {
            "command-unavailable"
        } else if let Err(reason) = words {
            reason
        } else {
            "command-unresolved"
        }),
    );
    if let Some(operation) = operation {
        fields.insert("santi_operation".into(), json!(operation));
        if let Some(response) = input.get("tool_response") {
            references(response, fields);
        }
    }
}

fn references(response: &Value, fields: &mut Map<String, Value>) {
    let text = response.to_string();
    let mut found = std::collections::BTreeSet::new();
    for token in
        text.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
    {
        let suffix = ["inbox_", "ss_", "turn_", "job_"]
            .iter()
            .find_map(|prefix| token.strip_prefix(prefix));
        if suffix.is_some_and(|suffix| {
            (20..=40).contains(&suffix.len()) && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            found.insert(token);
            if found.len() == 16 {
                break;
            }
        }
    }
    if !found.is_empty() {
        fields.insert("references_limited".into(), json!(found.len() == 16));
        fields.insert("santi_references".into(), json!(found));
        fields.insert("reference_basis".into(), json!("tool-output-candidates"));
    }
}

pub(super) fn operation(words: &[String]) -> Option<&'static str> {
    let words = match words {
        [program, seat, rest @ ..] if name(program) == "runseal" && seat.starts_with(':') => rest,
        rest => rest,
    };
    let (program, args) = words.split_first()?;
    if name(program) != "santi" {
        return None;
    }
    if args
        .iter()
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| matches!(arg.as_str(), "--help" | "--version" | "-h" | "-V"))
    {
        return Some("help");
    }
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["health", ..] => Some("health"),
        ["receipt", ..] => Some("receipt"),
        ["strand", "send", ..] => Some("strand.send"),
        ["strand", "create", ..] => Some("strand.create"),
        ["strand", "runtime", ..] => Some("strand.runtime"),
        ["strand", "execution-tail", ..] => Some("strand.execution-tail"),
        ["strand", "activity", ..] => Some("strand.activity"),
        ["strand", "list", ..] => Some("strand.list"),
        ["turn", "show", ..] => Some("turn.show"),
        ["turn", "stop", ..] => Some("turn.stop"),
        ["job", "get", ..] => Some("job.get"),
        ["job", "query", ..] => Some("job.query"),
        _ => None,
    }
}

fn name(program: &str) -> &str {
    Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
}
