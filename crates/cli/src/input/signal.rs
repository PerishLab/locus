use signal_hook::consts::{SIGINT, SIGTERM};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

const GRACE: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(20);

static RECEIVED: OnceLock<Arc<AtomicUsize>> = OnceLock::new();

pub(crate) fn watch() {
    let received = RECEIVED.get_or_init(Arc::default);
    for signal in [SIGINT, SIGTERM] {
        let value = usize::try_from(signal).unwrap_or_default();
        if let Err(error) = signal_hook::flag::register_usize(signal, Arc::clone(received), value) {
            eprintln!("locus: cannot handle signal {signal}: {error}");
        }
    }
    let received = Arc::clone(received);
    thread::spawn(move || {
        while received.load(Ordering::SeqCst) == 0 {
            thread::sleep(POLL);
        }
        thread::sleep(GRACE);
        std::process::exit(code(received.load(Ordering::SeqCst)).into());
    });
}

pub(crate) fn received() -> Option<u8> {
    let signal = RECEIVED.get()?.load(Ordering::SeqCst);
    (signal != 0).then(|| code(signal))
}

fn code(signal: usize) -> u8 {
    u8::try_from(signal).map_or(u8::MAX, |signal| signal.saturating_add(128))
}
