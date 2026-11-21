use super::{delivery, launcher};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) struct Worker(Arc<Pump>);

struct Pump {
    directory: PathBuf,
    sender: Option<PathBuf>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    dirty: bool,
    scheduled: bool,
    result: Option<Receiver<()>>,
}

impl Worker {
    pub(super) fn new(directory: PathBuf, sender: Option<PathBuf>) -> Self {
        Self(Arc::new(Pump {
            directory,
            sender,
            state: Mutex::default(),
        }))
    }

    pub(super) fn published(&self) {
        let Ok(mut state) = self.0.state.lock() else {
            return;
        };
        state.dirty = true;
        self.0.schedule(&mut state);
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
        if self.0.sender.is_none()
            && let Some(result) = result
        {
            let _ = result.recv_timeout(Duration::from_millis(300));
        }
        Ok(())
    }
}

impl Pump {
    fn schedule(self: &Arc<Self>, state: &mut State) {
        if state.scheduled {
            return;
        }
        let this = Arc::downgrade(self);
        state.scheduled = std::thread::Builder::new()
            .name("locus-wake".into())
            .spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
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
            self.schedule(&mut state);
        } else {
            let _ = self.launch(&mut state);
        }
    }

    fn launch(&self, state: &mut State) -> Result<(), String> {
        let (done, result) = channel();
        if let Some(path) = &self.sender {
            launcher::spawn(path, &self.directory, done)
                .map_err(|_| "cannot start configured HTTPS sender")?;
        } else {
            let path = self.directory.clone();
            std::thread::Builder::new()
                .name("locus-delivery".into())
                .spawn(move || {
                    let _ = delivery::send(&path);
                    let _ = done.send(());
                })
                .map_err(|_| "cannot start in-process HTTPS sender")?;
        }
        state.dirty = false;
        state.result = Some(result);
        Ok(())
    }
}
