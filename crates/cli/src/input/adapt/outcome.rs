use locus_derivation::quality::{Attempt, Issue, Stage, key};
use serde_json::Value;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct Fault {
    pub code: &'static str,
    pub field: Option<&'static str>,
}

impl From<&'static str> for Fault {
    fn from(code: &'static str) -> Self {
        Self { code, field: None }
    }
}

impl Fault {
    pub fn field(field: &'static str) -> Self {
        Self {
            code: "field.invalid",
            field: Some(field),
        }
    }

    pub fn wire(error: serde_json::Error) -> Self {
        let message = error.to_string();
        let field = [
            "session_id",
            "turn_id",
            "tool_use_id",
            "tool_name",
            "reason",
            "source",
            "agent_id",
            "agent_type",
            "trigger",
        ]
        .into_iter()
        .find(|name| message.starts_with(&format!("missing field `{name}`")));
        Self {
            code: "field.invalid",
            field,
        }
    }

    pub fn issue(self, stage: Stage) -> Issue {
        Issue {
            stage,
            code: self.code.into(),
            field: self.field.map(str::to_owned),
        }
    }
}

pub(super) fn start() -> Attempt {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| u64::try_from(time.as_nanos()).ok());
    let mut bytes = [0_u8; 16];
    let id = getrandom::fill(&mut bytes)
        .ok()
        .map(|()| bytes.iter().map(|byte| format!("{byte:02x}")).collect());
    Attempt {
        schema: "locus.adapter.attempt/v1".into(),
        at,
        id,
        ..Attempt::default()
    }
}

pub(super) fn identify(attempt: &mut Attempt, bytes: &[u8]) {
    let Ok(input) = serde_json::from_slice::<Value>(bytes) else {
        return;
    };
    attempt.group = input
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|text| key(text))
        .map(str::to_owned);
    let name = input.get("hook_event_name").and_then(Value::as_str);
    attempt.event = name
        .filter(|name| {
            matches!(
                *name,
                "SessionStart"
                    | "SessionEnd"
                    | "UserPromptSubmit"
                    | "Stop"
                    | "Interrupt"
                    | "PreToolUse"
                    | "PostToolUse"
                    | "PermissionRequest"
                    | "PreCompact"
                    | "PostCompact"
                    | "SubagentStart"
                    | "SubagentStop"
                    | "PreModelRequest"
                    | "PostModelRequest"
            )
        })
        .map(str::to_owned);
}

pub(super) fn emit(attempt: &Attempt) {
    let Ok(mut record) = serde_json::to_vec(attempt) else {
        return;
    };
    record.push(b'\n');
    let _ = io::stderr().lock().write_all(&record);
}
