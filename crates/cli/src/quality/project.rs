use locus::Atom;
use locus_derivation::quality::{Fact, Side};

pub(super) fn fact(atom: Atom) -> Result<Fact, String> {
    if atom.producer() != Some("locus-adapter-codex") {
        return Err("API returned an unexpected producer".into());
    }
    let payload = atom.payload().ok_or("adapter Atom has no payload")?;
    if payload.get("schema").and_then(serde_json::Value::as_str) != Some("locus.adapter.codex/v1") {
        return Err("unsupported adapter Atom schema".into());
    }
    let event = payload
        .get("event")
        .and_then(serde_json::Value::as_str)
        .ok_or("missing adapter event")?;
    let side = match event {
        "PreToolUse" => Some(Side::Before),
        "PostToolUse" => Some(Side::After),
        "SessionStart" | "SessionEnd" | "UserPromptSubmit" | "Stop" | "Interrupt"
        | "PermissionRequest" | "PreCompact" | "PostCompact" | "SubagentStart" | "SubagentStop" => {
            None
        }
        _ => return Err("unsupported adapter event".into()),
    };
    let class = match payload
        .get("tool_class")
        .and_then(serde_json::Value::as_str)
    {
        Some("santi") => {
            let profile = payload
                .get("classification_profile")
                .and_then(serde_json::Value::as_str);
            Some(
                if profile == Some("santi.command/simple-v1") {
                    "santi"
                } else {
                    "santi-unqualified"
                }
                .into(),
            )
        }
        Some("unclassified") => Some("unclassified".into()),
        Some(_) => Some("unrecognized".into()),
        None => None,
    };
    if side.is_some()
        && (!atom.context().contains_key("codex.turn")
            || !atom.context().contains_key("codex.tool"))
    {
        return Err("tool event lacks its required correlation scope".into());
    }
    Ok(Fact {
        at: atom.at(),
        id: atom
            .id()
            .ok_or("adapter Atom has no acceptance identity")?
            .into(),
        group: atom
            .context()
            .get("codex.session")
            .ok_or("adapter Atom has no session")?
            .clone(),
        round: atom.context().get("codex.turn").cloned(),
        call: atom.context().get("codex.tool").cloned(),
        side,
        class,
        event: event.into(),
    })
}
