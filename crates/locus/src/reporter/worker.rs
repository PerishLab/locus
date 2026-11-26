use super::delivery::{self, Route};
use super::launcher;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) struct Worker(Arc<Pump>);

enum Mode {
    Local,
    Helper(PathBuf),
    Direct(Route),
}

struct Pump {
    directory: PathBuf,
    mode: Mode,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    dirty: bool,
    scheduled: bool,
    backoff: u64,
    result: Option<Receiver<()>>,
}

impl Worker {
    pub(super) fn new(directory: PathBuf, sender: Option<PathBuf>) -> Self {
        let mode = match sender {
            Some(path) => Mode::Helper(path),
            None => Mode::Local,
        };
        Self(Arc::new(Pump {
            directory,
            mode,
            state: Mutex::default(),
        }))
    }

    pub(super) fn direct(directory: PathBuf, route: Route) -> Self {
        Self(Arc::new(Pump {
            directory,
            mode: Mode::Direct(route),
            state: Mutex::default(),
        }))
    }

    pub(super) fn published(&self) {
        let Ok(mut state) = self.0.state.lock() else {
            return;
        };
        state.dirty = true;
        self.0.schedule(&mut state, Duration::from_millis(250));
    }

    pub(super) fn finish(&self) -> Result<(), String> {
        let result = {
            let mut state = self
                .0
                .state
                .lock()
                .map_err(|_| "HTTPS sender lock failed")?;
            if state.dirty {
                self.0.launch(&mut state)?;
            }
            state.result.take()
        };
        if !matches!(self.0.mode, Mode::Helper(_))
            && let Some(result) = result
        {
            let _ = result.recv_timeout(Duration::from_millis(300));
        }
        Ok(())
    }
}

impl Pump {
    fn schedule(self: &Arc<Self>, state: &mut State, delay: Duration) {
        if state.scheduled {
            return;
        }
        let this = Arc::downgrade(self);
        state.scheduled = std::thread::Builder::new()
            .name("locus-wake".into())
            .spawn(move || {
                std::thread::sleep(delay);
                if let Some(pump) = this.upgrade() {
                    pump.wake();
                }
            })
            .is_ok();
    }

    fn wake(self: &Arc<Self>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.scheduled = false;
        if !state.dirty {
            return;
        }
        if state
            .result
            .as_ref()
            .is_some_and(|done| done.try_recv() == Err(TryRecvError::Empty))
        {
            self.schedule(&mut state, Duration::from_millis(250));
        } else {
            let _ = self.launch(&mut state);
        }
    }

    fn settled(self: &Arc<Self>, outcome: Result<bool, String>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.result = None;
        match outcome {
            Ok(false) => {
                state.backoff = 0;
                if state.dirty {
                    self.schedule(&mut state, Duration::from_millis(250));
                }
            }
            Ok(true) => {
                state.backoff = 0;
                state.dirty = true;
                self.schedule(&mut state, Duration::from_millis(10));
            }
            Err(_) => {
                let delay = if state.backoff == 0 {
                    1
                } else {
                    (state.backoff * 2).min(30)
                };
                state.backoff = delay;
                state.dirty = true;
                self.schedule(&mut state, Duration::from_secs(delay));
            }
        }
    }

    fn launch(self: &Arc<Self>, state: &mut State) -> Result<(), String> {
        let (done, result) = channel();
        match &self.mode {
            Mode::Helper(path) => {
                launcher::spawn(path, &self.directory, done)
                    .map_err(|_| "cannot start configured HTTPS sender")?;
            }
            Mode::Local => {
                let path = self.directory.clone();
                std::thread::Builder::new()
                    .name("locus-delivery".into())
                    .spawn(move || {
                        let _ = delivery::send(&path);
                        let _ = done.send(());
                    })
                    .map_err(|_| "cannot start in-process HTTPS sender")?;
            }
            Mode::Direct(route) => {
                self.forward(route, done)?;
            }
        }
        state.dirty = false;
        state.result = Some(result);
        Ok(())
    }

    fn forward(self: &Arc<Self>, route: &Route, done: Sender<()>) -> Result<(), String> {
        let path = self.directory.clone();
        let route = route.clone();
        let this = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("locus-delivery".into())
            .spawn(move || {
                let outcome = delivery::push(&path, &route);
                let _ = done.send(());
                if let Some(pump) = this.upgrade() {
                    pump.settled(outcome);
                }
            })
            .map_err(|_| "cannot start in-process HTTPS sender")?;
        Ok(())
    }
}
