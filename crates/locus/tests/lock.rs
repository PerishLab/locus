use locus::reporter;
use locus::{Candidate, Config, Context, Engine, Policy};
use radicula_lock::Mode;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn held() {
    for mode in [Mode::Exclusive, Mode::Shared] {
        let directory = temp("held");
        let endpoint = closed();
        let policy = Policy::default().reporter(reporter::Spec::api(&endpoint, &*directory));
        let engine = Engine::bootstrap(Config::new(policy)).expect("engine");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.join("spool.lock"))
            .expect("lock file");
        radicula_lock::lock(&lock, mode).expect("hold");
        let (sender, receiver) = mpsc::channel();
        let writer = thread::spawn(move || {
            engine
                .append(
                    &Context::empty(),
                    Candidate::event(json!({"event": "held"})),
                )
                .expect("append");
            sender.send(()).expect("done");
        });
        assert!(receiver.recv_timeout(Duration::from_millis(400)).is_err());
        assert!(!directory.join("active.jsonl").exists());
        radicula_lock::unlock(&lock).expect("release");
        receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("writer resumes");
        writer.join().expect("writer");
        let active = fs::read_to_string(directory.join("active.jsonl")).expect("active");
        assert_eq!(active.lines().count(), 1);
    }
}

#[test]
fn released() {
    let directory = temp("released");
    let endpoint = closed();
    let policy = Policy::default().reporter(reporter::Spec::api(&endpoint, &*directory));
    let engine = Engine::bootstrap(Config::new(policy)).expect("engine");
    for sequence in 0..3 {
        engine
            .append(
                &Context::empty(),
                Candidate::event(json!({"sequence": sequence})),
            )
            .expect("append");
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join("spool.lock"))
        .expect("lock file");
    assert!(radicula_lock::attempt(&lock, Mode::Exclusive).is_ok());
    radicula_lock::unlock(&lock).expect("release");
}

fn closed() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listen");
    format!("http://{}", listener.local_addr().expect("address"))
}

fn temp(label: &str) -> Temp {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "locus-spool-lock-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("temp");
    Temp(path)
}

struct Temp(PathBuf);

impl std::ops::Deref for Temp {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
