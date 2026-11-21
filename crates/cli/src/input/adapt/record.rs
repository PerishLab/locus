use super::{codex::Event, outcome::Fault};
use locus_derivation::locus::{
    self, Candidate, Config, Context, Engine, Hook, Key, Observation, Policy, Role, Status,
};
use locus_derivation::quality::{Attempt, Buffer, Issue, Progress, Stage};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    atom: Option<String>,
    buffered: Option<bool>,
    issues: Vec<Issue>,
}

#[derive(Default)]
struct Diagnostic(Mutex<State>);

impl Hook for Diagnostic {
    fn observe(&self, observation: &Observation) {
        let Ok(mut state) = self.0.lock() else {
            return;
        };
        match observation {
            Observation::Report(outcome) => {
                state.atom = outcome.atom().id().map(str::to_owned);
                state.buffered = match outcome.status() {
                    Status::Delivered => Some(true),
                    Status::Failed => Some(false),
                    Status::Gated => None,
                };
                if outcome.error().is_some() {
                    state
                        .issues
                        .push(Fault::from("reporter.failed").issue(Stage::Buffer));
                }
            }
            Observation::Diagnostic(diagnostic) if state.issues.len() < 7 => {
                let (stage, code) = match diagnostic.code() {
                    "reporter.registration" => (Stage::Bootstrap, "reporter.registration"),
                    "bootstrap.failed" => (Stage::Bootstrap, "bootstrap.failed"),
                    "reporter.loss" => (Stage::Buffer, "reporter.loss"),
                    "clock.failed" => (Stage::Accept, "clock.failed"),
                    _ => (Stage::Accept, "engine.diagnostic"),
                };
                state.issues.push(Fault::from(code).issue(stage));
            }
            _ => {}
        }
    }
}

impl Diagnostic {
    fn settle(&self, attempt: &mut Attempt) {
        if let Ok(mut guard) = self.0.lock() {
            let state = std::mem::take(&mut *guard);
            attempt.atom.clone_from(&state.atom);
            if matches!(attempt.outcome, Progress::Accepted { .. }) {
                let buffer = match state.buffered {
                    Some(true) => Buffer::Written,
                    Some(false) => Buffer::Failed,
                    None => Buffer::Unknown,
                };
                attempt.outcome = Progress::Accepted { buffer };
            }
            attempt.issues.extend(state.issues.clone());
        } else {
            attempt
                .issues
                .push(Fault::from("diagnostic.capture").issue(Stage::Accept));
        }
    }
}

pub(super) struct Target {
    pub(super) buffer: PathBuf,
    pub(super) endpoint: String,
    pub(super) credential: Option<PathBuf>,
}

impl Target {
    fn reporter(&self) -> Result<locus::reporter::Spec, Issue> {
        if let Some(key) = &self.credential {
            let spec = locus::reporter::Spec::https(&self.endpoint, key, &self.buffer);
            return Ok(spec.installed());
        }
        self.endpoint
            .strip_prefix("http://")
            .and_then(|authority| authority.parse::<std::net::SocketAddr>().ok())
            .ok_or_else(|| Fault::from("endpoint.numeric").issue(Stage::Bootstrap))?;
        Ok(locus::reporter::Spec::api(&self.endpoint, &self.buffer))
    }
}

pub(super) struct Session {
    engine: Result<Engine, Issue>,
    observer: Arc<Diagnostic>,
}

impl Session {
    pub(super) fn new(target: &Target) -> Self {
        let observer = Arc::new(Diagnostic::default());
        let engine = target.reporter().and_then(|spec| {
            let policy = Policy::default()
                .producer("locus-adapter-codex")
                .reporter(spec);
            Engine::bootstrap(Config::new(policy).hook(observer.clone()))
                .map_err(|_| Fault::from("engine.bootstrap").issue(Stage::Bootstrap))
        });
        Self { engine, observer }
    }

    pub(super) fn append(&self, event: Event, attempt: &mut Attempt) {
        let result = self
            .engine
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|engine| collect(engine, event));
        attempt.outcome = match &result {
            Ok(()) => Progress::Accepted {
                buffer: Buffer::Unknown,
            },
            Err(issue) => Progress::Rejected {
                stage: issue.stage.clone(),
            },
        };
        self.observer.settle(attempt);
        if let Err(issue) = result {
            attempt.issues.push(issue);
        }
    }
}

fn collect(engine: &Engine, event: Event) -> Result<(), Issue> {
    let mut candidate = Candidate::event(
        event
            .payload
            .clone()
            .unwrap_or_else(|| event.fields.clone().into()),
    );
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
    for (role, key) in event.roles {
        candidate = bind(candidate, role, key)?;
    }
    engine
        .append(&Context::empty(), candidate)
        .map_err(|_| Fault::from("engine.append").issue(Stage::Accept))?;
    Ok(())
}

fn bind(candidate: Candidate, role: &str, key: &str) -> Result<Candidate, Issue> {
    Ok(candidate.explicit(
        Role::new(role).map_err(|_| Fault::from("context.role").issue(Stage::Accept))?,
        Key::new(key).map_err(|_| Fault::from("context.key").issue(Stage::Accept))?,
    ))
}
