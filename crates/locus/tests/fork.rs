use locus::{Atom, Config, Context, Engine, Hook, Kind, Observation, Policy, Role};
use std::sync::{Arc, Mutex};

type View<'a> = Option<(&'a Engine, &'a Context)>;

#[derive(Default)]
struct Capture {
    seen: Mutex<Vec<Atom>>,
}

impl Capture {
    fn atoms(&self) -> Vec<Atom> {
        std::mem::take(&mut *self.seen.lock().expect("capture"))
    }
}

impl Hook for Capture {
    fn observe(&self, observation: &Observation) {
        if let Observation::Report(outcome) = observation {
            self.seen
                .lock()
                .expect("capture")
                .push(outcome.atom().clone());
        }
    }
}

#[locus::trace(with = view, frame = frame)]
fn outer(view: View<'_>) -> Option<Context> {
    let (engine, _) = view?;
    let forked = engine.fork(frame.as_ref()?).expect("fork");
    worker(Some((engine, &forked)));
    frame
}

#[locus::trace(with = view)]
fn worker(view: View<'_>) {
    let _ = view;
}

#[locus::trace(with = view, frame = frame)]
fn muted(view: View<'_>) -> bool {
    let _ = view;
    frame.is_none()
}

#[test]
fn fork() {
    let (engine, capture) = engine();
    let root = Context::empty();
    let frame = outer(Some((&engine, &root))).expect("frame");
    let atoms = capture.atoms();
    assert_eq!(atoms.len(), 4);
    let span = frame.read(&Role::span()).expect("span").text();
    let trace = frame.read(&Role::trace()).expect("trace").text();
    assert_eq!(atoms[0].context()["locus.span"], span);
    assert_eq!(atoms[3].context(), atoms[0].context());
    assert!(!atoms[0].context().contains_key("locus.parent"));
    assert_eq!(atoms[1].context()["locus.parent"], span);
    assert_ne!(atoms[1].context()["locus.trace"], trace);
    assert_ne!(atoms[1].context()["locus.span"], span);
    assert_eq!(atoms[2].context(), atoms[1].context());
}

#[test]
fn unframed() {
    let (engine, _) = engine();
    let error = engine
        .fork(&Context::empty())
        .expect_err("fork without a span must refuse");
    assert_eq!(error.kind(), &Kind::Context);
}

#[test]
fn silent() {
    assert!(muted(None));
}

fn engine() -> (Engine, Arc<Capture>) {
    let capture = Arc::new(Capture::default());
    let engine =
        Engine::bootstrap(Config::new(Policy::default()).hook(capture.clone())).expect("bootstrap");
    (engine, capture)
}
