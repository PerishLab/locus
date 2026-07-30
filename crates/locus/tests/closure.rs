use locus::generator;
use locus::reporter;
use locus::{
    Candidate, Config, Context, Engine, Hook, Key, Kind, Observation, Origin, Policy, Role, Status,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default)]
struct Capture {
    seen: Mutex<Vec<Observation>>,
}

impl Capture {
    fn take(&self) -> Vec<Observation> {
        std::mem::take(&mut *self.seen.lock().expect("capture"))
    }
}

impl Hook for Capture {
    fn observe(&self, observation: &Observation) {
        self.seen.lock().expect("capture").push(observation.clone());
    }
}

#[test]
fn cascade() {
    let hook = Arc::new(Capture::default());
    let engine =
        Engine::bootstrap(Config::new(Policy::default()).hook(hook.clone())).expect("bootstrap");
    let trace = Role::trace();
    let first = Key::new("trace-a").expect("key");
    let second = Key::new("trace-b").expect("key");
    let root = Context::empty();

    let accepted = engine
        .append(
            &root,
            Candidate::context().explicit(trace.clone(), first.clone()),
        )
        .expect("explicit");
    let context = accepted.context();
    assert_eq!(root.read(&trace), None);
    assert_eq!(context.read(&trace), Some(&first));

    engine
        .append(
            &context,
            Candidate::event(json!({"event": "inherit"})).ensure(trace.clone()),
        )
        .expect("inherit");
    let changed = engine
        .append(
            &context,
            Candidate::event(json!({"event": "override"})).explicit(trace.clone(), second.clone()),
        )
        .expect("override")
        .context();

    assert_eq!(context.read(&trace), Some(&first));
    assert_eq!(changed.read(&trace), Some(&second));
    let seen = hook.take();
    let origins: Vec<Origin> = seen
        .iter()
        .filter_map(report)
        .flat_map(|outcome| outcome.atom().choices())
        .map(|choice| choice.origin().clone())
        .collect();
    assert_eq!(
        origins,
        vec![Origin::Explicit, Origin::Inherited, Origin::Explicit]
    );
}

#[test]
fn random() {
    let hook = Arc::new(Capture::default());
    let engine =
        Engine::bootstrap(Config::new(Policy::default()).hook(hook.clone())).expect("bootstrap");
    let trace = Role::trace();
    let context = engine
        .append(
            &Context::empty(),
            Candidate::context().ensure(trace.clone()),
        )
        .expect("append")
        .context();
    let key = context.read(&trace).expect("generated");
    assert_eq!(key.text().len(), 32);
    let seen = hook.take();
    assert_eq!(
        report(&seen[0]).expect("outcome").atom().choices()[0].origin(),
        &Origin::Generated
    );
}

#[test]
fn shared() {
    let home = temp("shared");
    fs::create_dir(&home).expect("temp");
    let path = home.join("trace.json");
    let trace = Role::trace();
    let policy = || Policy::default().generator(trace.clone(), generator::Spec::shared(&path));
    let first = Engine::bootstrap(Config::new(policy())).expect("first");
    let second = Engine::bootstrap(Config::new(policy())).expect("second");

    let one = first
        .append(
            &Context::empty(),
            Candidate::context().ensure(trace.clone()),
        )
        .expect("one")
        .context();
    let two = second
        .append(
            &Context::empty(),
            Candidate::context().ensure(trace.clone()),
        )
        .expect("two")
        .context();

    assert_eq!(one.read(&trace), two.read(&trace));
    let stored: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
    assert_eq!(stored["version"], 1);
    fs::remove_dir_all(home).expect("cleanup");
}

#[test]
fn malformed() {
    let home = temp("broken");
    fs::create_dir(&home).expect("temp");
    let path = home.join("trace.json");
    fs::write(&path, "broken\n").expect("write");
    let trace = Role::trace();
    let hook = Arc::new(Capture::default());
    let policy = Policy::default().generator(trace.clone(), generator::Spec::shared(&path));
    let engine = Engine::bootstrap(Config::new(policy).hook(hook.clone())).expect("bootstrap");

    let error = engine
        .append(&Context::empty(), Candidate::context().ensure(trace))
        .expect_err("refuse");
    assert_eq!(error.kind(), &Kind::Generator);
    let seen = hook.take();
    let diagnostic = match &seen[0] {
        Observation::Diagnostic(diagnostic) => diagnostic,
        Observation::Report(_) => panic!("expected diagnostic"),
    };
    assert_eq!(diagnostic.code(), "generator.failed");
    assert_eq!(fs::read_to_string(&path).expect("read"), "broken\n");
    fs::remove_dir_all(home).expect("cleanup");
}

#[test]
fn bootstrap() {
    let hook = Arc::new(Capture::default());
    let policy = Policy::default().fallback(generator::Spec::new("unknown", json!({})));
    let error = Engine::bootstrap(Config::new(policy).hook(hook.clone()))
        .err()
        .expect("refuse");

    assert_eq!(error.kind(), &Kind::Config);
    let seen = hook.take();
    let diagnostic = match &seen[0] {
        Observation::Diagnostic(diagnostic) => diagnostic,
        Observation::Report(_) => panic!("expected diagnostic"),
    };
    assert_eq!(diagnostic.code(), "bootstrap.failed");
    assert!(diagnostic.message().contains("unknown generator"));
}

#[test]
fn file() {
    let home = temp("report");
    fs::create_dir(&home).expect("temp");
    let path = home.join("atoms.jsonl");
    let hook = Arc::new(Capture::default());
    let policy = Policy::default().reporter(reporter::Spec::file(&path));
    let engine = Engine::bootstrap(Config::new(policy).hook(hook.clone())).expect("bootstrap");

    engine
        .append(
            &Context::empty(),
            Candidate::event(json!({"event": "cli.start"})).ensure(Role::trace()),
        )
        .expect("append");

    let text = fs::read_to_string(&path).expect("read");
    assert_eq!(text.lines().count(), 1);
    let atom: locus::Atom = serde_json::from_str(text.trim()).expect("atom");
    assert_eq!(atom.payload(), Some(&json!({"event": "cli.start"})));
    let seen = hook.take();
    let outcome = report(&seen[0]).expect("outcome");
    assert_eq!(outcome.status(), &Status::Delivered);
    assert_eq!(outcome.reporter(), Some("file"));
    fs::remove_dir_all(home).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn failure() {
    let hook = Arc::new(Capture::default());
    let policy = Policy::default().reporter(reporter::Spec::file("/dev/full"));
    let engine = Engine::bootstrap(Config::new(policy).hook(hook.clone())).expect("bootstrap");

    let accepted = engine
        .append(
            &Context::empty(),
            Candidate::event(json!({"event": "still-fact"})),
        )
        .expect("accepted");

    assert_eq!(accepted.context(), Context::empty());
    let seen = hook.take();
    assert_eq!(report(&seen[0]).expect("outcome").status(), &Status::Failed);
}

struct Panic;

impl Hook for Panic {
    fn observe(&self, observation: &Observation) {
        let _ = observation;
        panic!("hook panic");
    }
}

#[test]
fn hook() {
    let engine =
        Engine::bootstrap(Config::new(Policy::default()).hook(Arc::new(Panic))).expect("bootstrap");
    engine
        .append(
            &Context::empty(),
            Candidate::event(json!({"event": "accepted"})),
        )
        .expect("append");
}

#[test]
fn source() {
    let hook = Arc::new(Capture::default());
    let engine =
        Engine::bootstrap(Config::new(Policy::default()).hook(hook.clone())).expect("bootstrap");
    locus::record!(
        &engine,
        &Context::empty(),
        Candidate::event(json!({"event": "macro"})),
    )
    .expect("record");

    let seen = hook.take();
    let source = report(&seen[0])
        .expect("outcome")
        .atom()
        .source()
        .expect("source");
    assert!(source.file().ends_with("closure.rs"));
    assert!(source.line() > 0);
}

#[test]
fn empty() {
    let engine = Engine::bootstrap(Config::new(Policy::default())).expect("bootstrap");
    let error = engine
        .append(&Context::empty(), Candidate::context())
        .expect_err("refuse");
    assert_eq!(error.kind(), &Kind::Record);
}

#[test]
fn serde() {
    assert!(serde_json::from_str::<Role>("\"\"").is_err());
    assert!(serde_json::from_str::<Key>("\"\\u0000\"").is_err());
}

fn report(observation: &Observation) -> Option<&locus::Outcome> {
    match observation {
        Observation::Report(outcome) => Some(outcome),
        Observation::Diagnostic(_) => None,
    }
}

fn temp(label: &str) -> PathBuf {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("locus-{label}-{}-{at}", std::process::id()))
}
