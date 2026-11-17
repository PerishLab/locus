use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

#[derive(Serialize)]
pub(super) struct Text(pub String);

impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.is_empty() || text.len() > 512 || text.chars().any(char::is_control) {
            return Err(serde::de::Error::custom("invalid bounded text"));
        }
        Ok(Self(text))
    }
}

#[derive(Deserialize)]
pub(super) struct Envelope {
    #[serde(rename = "session_id")]
    pub session: Text,
    #[serde(flatten)]
    pub event: Event,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "hook_event_name")]
pub(super) enum Event {
    #[serde(rename = "SessionStart")]
    Start { source: Text },
    #[serde(rename = "SessionEnd")]
    End { reason: Text },
    #[serde(rename = "UserPromptSubmit")]
    Submit(Round),
    #[serde(rename = "Stop")]
    Stop(Round),
    #[serde(rename = "Interrupt")]
    Interrupt(Round),
    #[serde(rename = "PreToolUse")]
    Before(Tool),
    #[serde(rename = "PostToolUse")]
    After(Tool),
    #[serde(rename = "PermissionRequest")]
    Permission(Permission),
    #[serde(rename = "PreCompact")]
    Compact(Compact),
    #[serde(rename = "PostCompact")]
    Compacted(Compact),
    #[serde(rename = "SubagentStart")]
    Spawn(Agent),
    #[serde(rename = "SubagentStop")]
    Join(Agent),
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Round {
    #[serde(rename = "turn_id")]
    turn: Text,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Tool {
    #[serde(rename = "turn_id")]
    turn: Text,
    #[serde(rename = "tool_name")]
    name: Text,
    #[serde(rename = "tool_use_id")]
    call: Text,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Permission {
    #[serde(rename = "turn_id")]
    turn: Text,
    #[serde(rename = "tool_name")]
    name: Text,
    #[serde(rename = "tool_use_id", skip_serializing_if = "Option::is_none")]
    call: Option<Text>,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Compact {
    #[serde(rename = "turn_id")]
    turn: Text,
    trigger: Text,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Agent {
    #[serde(rename = "turn_id")]
    turn: Text,
    #[serde(rename = "agent_id")]
    agent: Text,
    #[serde(rename = "agent_type")]
    kind: Text,
}

impl Event {
    pub fn shell(&self) -> bool {
        matches!(self, Self::Before(_) | Self::After(_) | Self::Permission(_))
    }

    pub fn fields(self) -> Result<Map<String, Value>, &'static str> {
        let value = serde_json::to_value(self).map_err(|_| "input.projection")?;
        let Value::Object(mut fields) = value else {
            return Err("input.projection");
        };
        let name = fields.remove("hook_event_name").ok_or("input.projection")?;
        fields.insert("event".into(), name);
        Ok(fields)
    }
}
