use locus::collector;
use locus::reporter;
use locus::{
    Candidate, Config, Context, Engine, Hook, Key, Kind, Observation, Origin, Policy, Role,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

struct Variable {
    name: String,
}

impl Variable {
    fn set(label: &str, value: &str) -> Self {
        let name = format!("LOCUS_COLLECTOR_{label}_{}", std::process::id());
        unsafe {
            std::env::set_var(&name, value);
        }
        Self { name }
    }
}

impl Drop for Variable {
    fn drop(&mut self) {
        unsafe {
            std::env::remove_var(&self.name);
        }
    }
}

#[derive(Default)]
struct Diagnostics {
    codes: Mutex<Vec<String>>,
}

impl Hook for Diagnostics {
    fn observe(&self, observation: &Observation) {
        if let Observation::Diagnostic(diagnostic) = observation {
            self.codes
                .lock()
                .expect("diagnostics")
                .push(diagnostic.code().into());
        }
    }
}

#[test]
fn environment() {
    let variable = Variable::set("TARGET", "repository-a");
    let target = Role::new("plumb.target").expect("role");
    let policy =
        Policy::default().collector("target", collector::Spec::environment(&variable.name, 64));
    let atom = record(
        "environment",
        policy,
        Candidate::event(json!({"event": "start"})).collect(target.clone(), "target"),
    );

    assert_eq!(atom.context()[target.text()], "repository-a");
    assert_eq!(atom.choices()[0].origin(), &Origin::Explicit);
    assert_eq!(atom.collections()[0].role(), target.text());
    assert_eq!(atom.collections()[0].binding(), "target");
    assert_eq!(atom.collections()[0].collector(), "environment");
    assert_eq!(atom.collections()[0].selector(), variable.name);
}

#[test]
fn fallback() {
    let target = Role::new("plumb.target").expect("role");
    let missing = format!("LOCUS_COLLECTOR_MISSING_{}", std::process::id());
    unsafe {
        std::env::remove_var(&missing);
    }
    let policy = Policy::default()
        .collector("target", collector::Spec::environment(missing, 64))
        .collector("target", collector::Spec::argument(255, 64))
        .collector("target", collector::Spec::process("id", 64));
    let atom = record(
        "fallback",
        policy,
        Candidate::event(json!({"event": "start"})).collect(target.clone(), "target"),
    );

    assert_eq!(
        atom.context()[target.text()],
        std::process::id().to_string()
    );
    assert_eq!(atom.collections()[0].collector(), "process");
    assert_eq!(atom.collections()[0].selector(), "id");
}

#[test]
fn explicit() {
    let variable = Variable::set("EXPLICIT", "");
    let target = Role::new("plumb.target").expect("role");
    let policy =
        Policy::default().collector("target", collector::Spec::environment(&variable.name, 64));
    let atom = record(
        "explicit",
        policy,
        Candidate::event(json!({"event": "start"}))
            .collect(target.clone(), "target")
            .explicit(target.clone(), Key::new("manual").expect("key")),
    );

    assert_eq!(atom.context()[target.text()], "manual");
    assert!(atom.collections().is_empty());
}

#[test]
fn invalid() {
    let variable = Variable::set("INVALID", "");
    let hook = Arc::new(Diagnostics::default());
    let target = Role::new("plumb.target").expect("role");
    let policy = Policy::default()
        .collector("target", collector::Spec::environment(&variable.name, 64))
        .collector("target", collector::Spec::process("id", 64));
    let engine =
        Engine::bootstrap(Config::new(policy).hook(hook.clone())).expect("bootstrap engine");
    let error = engine
        .append(
            &Context::empty(),
            Candidate::event(json!({"event": "start"})).collect(target, "target"),
        )
        .expect_err("invalid collection must refuse");

    assert_eq!(error.kind(), &Kind::Collector);
    assert_eq!(
        hook.codes.lock().expect("diagnostics").as_slice(),
        ["collector.failed"]
    );
}

#[test]
fn schema() {
    let policy = Policy::default().collector("target", collector::Spec::process("parent", 64));
    let error = Engine::bootstrap(Config::new(policy))
        .err()
        .expect("unknown selector must refuse");

    assert_eq!(error.kind(), &Kind::Config);
}

fn record(label: &str, policy: Policy, candidate: Candidate) -> locus::Atom {
    let home = temp(label);
    fs::create_dir(&home).expect("temp");
    let path = home.join("atoms.jsonl");
    let engine = Engine::bootstrap(Config::new(policy.reporter(reporter::Spec::file(&path))))
        .expect("bootstrap");
    engine.append(&Context::empty(), candidate).expect("append");
    let text = fs::read_to_string(path).expect("read");
    let atom = serde_json::from_str(text.trim()).expect("atom");
    fs::remove_dir_all(home).expect("cleanup");
    atom
}

fn temp(label: &str) -> PathBuf {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("locus-collector-{label}-{at}"))
}
