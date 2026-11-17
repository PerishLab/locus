use super::codex::Event;
use locus::{Candidate, Config, Context, Engine, Hook, Key, Observation, Policy, Role};
use std::path::PathBuf;
use std::sync::Arc;

struct Diagnostic;

impl Hook for Diagnostic {
    fn observe(&self, observation: &Observation) {
        match observation {
            Observation::Diagnostic(diagnostic) => {
                super::diagnose(diagnostic.code());
            }
            Observation::Report(outcome) if outcome.error().is_some() => {
                super::diagnose("reporter.failed");
            }
            _ => {}
        }
    }
}

pub(super) fn append(event: Event, buffer: PathBuf, endpoint: String) -> Result<(), &'static str> {
    endpoint
        .strip_prefix("http://")
        .and_then(|authority| authority.parse::<std::net::SocketAddr>().ok())
        .ok_or("endpoint.numeric")?;
    let policy = Policy::default()
        .producer("locus-adapter-codex")
        .reporter(locus::reporter::Spec::api(endpoint, buffer));
    let engine = Engine::bootstrap(Config::new(policy).hook(Arc::new(Diagnostic)))
        .map_err(|_| "engine.bootstrap")?;
    let mut candidate = Candidate::event(event.fields.clone().into());
    candidate = bind(candidate, "codex.session", &event.session)?;
    for (field, role) in [
        ("turn_id", "codex.turn"),
        ("tool_use_id", "codex.tool"),
        ("agent_id", "codex.agent"),
    ] {
        if let Some(value) = event.fields.get(field).and_then(serde_json::Value::as_str) {
            candidate = bind(candidate, role, value)?;
        }
    }
    engine
        .append(&Context::empty(), candidate)
        .map_err(|_| "engine.append")?;
    Ok(())
}

fn bind(candidate: Candidate, role: &str, key: &str) -> Result<Candidate, &'static str> {
    Ok(candidate.explicit(
        Role::new(role).map_err(|_| "context.role")?,
        Key::new(key).map_err(|_| "context.key")?,
    ))
}
