use locus::reporter;
use locus::{Candidate, Config, Context, Engine, Hook, Observation, Policy};
use serde_json::json;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default)]
struct Capture {
    codes: Mutex<Vec<String>>,
}

impl Hook for Capture {
    fn observe(&self, observation: &Observation) {
        if let Observation::Diagnostic(diagnostic) = observation {
            self.codes
                .lock()
                .expect("capture")
                .push(diagnostic.code().to_string());
        }
    }
}

#[test]
fn registered() {
    let directory = temp("registered");
    let (endpoint, requests) = listen(1);
    let (engine, capture) = engine(&endpoint, &directory, Some("concord"));
    let request = requests.recv().expect("registration");
    assert!(request.starts_with("POST /api/v1/spools HTTP/1.1"));
    let canonical = fs::canonicalize(&directory).expect("canonical");
    let body: serde_json::Value =
        serde_json::from_str(request.rsplit("\r\n\r\n").next().expect("body")).expect("json");
    assert_eq!(body["producer"], "concord");
    assert_eq!(body["path"], canonical.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(directory.join("registration")).expect("marker"),
        endpoint
    );
    emit(&engine);
    assert_eq!(lines(&directory.join("active.jsonl")), 1);
    assert!(capture.codes.lock().expect("capture").is_empty());
}

#[test]
fn absent() {
    let directory = temp("absent");
    let endpoint = closed();
    let (engine, capture) = engine(&endpoint, &directory, Some("concord"));
    emit(&engine);
    assert_eq!(lines(&directory.join("active.jsonl")), 1);
    assert_eq!(
        *capture.codes.lock().expect("capture"),
        vec!["reporter.registration"]
    );
}

#[test]
fn remembered() {
    let directory = temp("remembered");
    let (endpoint, requests) = listen(1);
    drop(engine(&endpoint, &directory, Some("concord")));
    requests.recv().expect("registration");
    let (engine, capture) = engine(&endpoint, &directory, Some("concord"));
    emit(&engine);
    assert!(capture.codes.lock().expect("capture").is_empty());
}

#[test]
fn anonymous() {
    let directory = temp("anonymous");
    let (endpoint, _requests) = listen(1);
    let (_engine, capture) = engine(&endpoint, &directory, None);
    assert_eq!(
        *capture.codes.lock().expect("capture"),
        vec!["reporter.registration"]
    );
}

#[test]
fn malformed() {
    let directory = temp("malformed");
    for endpoint in ["https://127.0.0.1:1", "127.0.0.1:1", "http://host/path"] {
        let policy = Policy::default().reporter(reporter::Spec::api(endpoint, &directory));
        assert!(Engine::bootstrap(Config::new(policy).hook(Arc::new(Capture::default()))).is_err());
    }
}

fn engine(endpoint: &str, directory: &Path, producer: Option<&str>) -> (Engine, Arc<Capture>) {
    let capture = Arc::new(Capture::default());
    let mut policy = Policy::default().reporter(reporter::Spec::api(endpoint, directory));
    if let Some(producer) = producer {
        policy = policy.producer(producer);
    }
    let engine = Engine::bootstrap(Config::new(policy).hook(capture.clone())).expect("engine");
    (engine, capture)
}

fn emit(engine: &Engine) {
    engine
        .append(&Context::empty(), Candidate::event(json!({"event": "api"})))
        .expect("append");
}

fn listen(count: usize) -> (String, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listen");
    let endpoint = format!("http://{}", listener.local_addr().expect("address"));
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for stream in listener.incoming().take(count) {
            let mut stream = stream.expect("stream");
            let request = request(&stream);
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n")
                .expect("reply");
            sender.send(request).expect("send");
        }
    });
    (endpoint, receiver)
}

fn request(stream: &TcpStream) -> String {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut head = String::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("line");
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().expect("length");
        }
        head.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("body");
    head + &String::from_utf8(body).expect("utf8")
}

fn closed() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listen");
    format!("http://{}", listener.local_addr().expect("address"))
}

fn lines(path: &Path) -> usize {
    fs::read_to_string(path).expect("active").lines().count()
}

fn temp(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "locus-api-reporter-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("temp");
    path
}
