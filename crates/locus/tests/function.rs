use locus::{Atom, Config, Context, Edge, Engine, Hook, Observation, Origin, Policy};
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::pin;
use std::sync::{Arc, Mutex};
use std::task::{Context as Task, Poll, Waker};

type View<'a> = Option<(&'a Engine, &'a Context)>;

#[derive(Clone, Copy)]
enum Mode {
    Error,
    Panic,
    Value,
}

#[derive(Default)]
struct Capture {
    seen: Mutex<Vec<Observation>>,
}

impl Capture {
    fn atoms(&self) -> Vec<Atom> {
        std::mem::take(&mut *self.seen.lock().expect("capture"))
            .into_iter()
            .filter_map(|observation| match observation {
                Observation::Report(outcome) => Some(outcome.atom().clone()),
                Observation::Diagnostic(_) => None,
            })
            .collect()
    }
}

impl Hook for Capture {
    fn observe(&self, observation: &Observation) {
        self.seen.lock().expect("capture").push(observation.clone());
    }
}

#[locus::trace(with = view)]
fn observed(view: View<'_>, mode: Mode) -> Result<usize, &'static str> {
    match mode {
        Mode::Error => return Err("expected"),
        Mode::Panic => panic!("expected"),
        Mode::Value => Ok(7),
    }
}

#[locus::trace(with = view)]
async fn asynchronous(view: View<'_>, pending: bool) -> usize {
    if pending {
        std::future::pending::<()>().await;
    }
    11
}

#[test]
fn function() {
    let (engine, capture) = engine();
    let context = Context::empty();
    assert_eq!(observed(Some((&engine, &context)), Mode::Value), Ok(7));

    let atoms = capture.atoms();
    assert_eq!(atoms.len(), 2);
    assert_eq!(edge(&atoms[0]), &Edge::Enter);
    assert_eq!(edge(&atoms[1]), &Edge::Return);
    assert_eq!(source(&atoms[0]).function(), Some("observed"));
    assert_eq!(atoms[0].context(), atoms[1].context());
    assert_eq!(atoms[0].choices().len(), 2);
    assert!(
        atoms[0]
            .choices()
            .iter()
            .all(|choice| choice.origin() == &Origin::Generated)
    );
    assert!(
        atoms[1]
            .choices()
            .iter()
            .all(|choice| choice.origin() == &Origin::Inherited)
    );
}

#[test]
fn error() {
    let (engine, capture) = engine();
    let context = Context::empty();
    assert_eq!(
        observed(Some((&engine, &context)), Mode::Error),
        Err("expected")
    );
    assert_eq!(capture.atoms().len(), 2);
}

#[test]
fn panic() {
    let (engine, capture) = engine();
    let context = Context::empty();
    let result = catch_unwind(AssertUnwindSafe(|| {
        observed(Some((&engine, &context)), Mode::Panic)
    }));
    assert!(result.is_err());

    let atoms = capture.atoms();
    assert_eq!(atoms.len(), 1);
    assert_eq!(edge(&atoms[0]), &Edge::Enter);
}

#[test]
fn muted() {
    let (engine, capture) = engine();
    assert_eq!(observed(None, Mode::Value), Ok(7));
    assert!(capture.atoms().is_empty());
    drop(engine);
}

#[test]
fn completion() {
    let (engine, capture) = engine();
    let context = Context::empty();
    assert_eq!(wait(asynchronous(Some((&engine, &context)), false)), 11);
    assert_eq!(capture.atoms().len(), 2);
}

#[test]
fn cancellation() {
    let (engine, capture) = engine();
    let context = Context::empty();
    {
        let future = asynchronous(Some((&engine, &context)), true);
        let mut future = pin!(future);
        let mut task = Task::from_waker(Waker::noop());
        assert_eq!(future.as_mut().poll(&mut task), Poll::Pending);
    }
    let atoms = capture.atoms();
    assert_eq!(atoms.len(), 1);
    assert_eq!(edge(&atoms[0]), &Edge::Enter);
}

fn engine() -> (Engine, Arc<Capture>) {
    let capture = Arc::new(Capture::default());
    let engine =
        Engine::bootstrap(Config::new(Policy::default()).hook(capture.clone())).expect("engine");
    (engine, capture)
}

fn source(atom: &Atom) -> &locus::Source {
    atom.source().expect("source")
}

fn edge(atom: &Atom) -> &Edge {
    source(atom).edge().expect("edge")
}

fn wait<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut task = Task::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut task) {
            return output;
        }
    }
}
