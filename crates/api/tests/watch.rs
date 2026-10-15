use locus::{Candidate, Config, Context, Engine, Policy, Role};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Nest(PathBuf);
struct Serve(Child, u16);
impl Nest {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "locus-api-watch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join(".locus")).unwrap();
        Self(root)
    }
    fn seat(&self) -> PathBuf {
        self.0.join(".locus")
    }
    fn configure(&self, text: &str) {
        std::fs::write(self.seat().join("config.toml"), text).unwrap();
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_locus-api"));
        command
            .env_clear()
            .env("HOME", &self.0)
            .env("USERPROFILE", &self.0)
            .env("LOCALAPPDATA", &self.0);
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
    fn serve(&self, port: u16) -> Serve {
        let listen = format!("127.0.0.1:{port}");
        let args = ["serve", "--interval", "20", "--listen", &listen, "--home"];
        let mut command = self.command();
        command.args(args).arg(self.seat()).stderr(Stdio::piped());
        let serve = Serve(command.spawn().unwrap(), port);
        wait(|| serve.get("/health").is_some());
        serve
    }
}
impl Serve {
    fn get(&self, path: &str) -> Option<String> {
        let mut stream = TcpStream::connect(("127.0.0.1", self.1)).ok()?;
        write!(stream, "GET {path} HTTP/1.0\r\n\r\n").ok()?;
        let mut text = String::new();
        stream.read_to_string(&mut text).ok()?;
        let (head, body) = text.split_once("\r\n\r\n")?;
        head.contains(" 200 ").then(|| body.to_string())
    }
    fn stored(&self, producer: &str) -> usize {
        let path = format!("/api/v1/atoms?producer={producer}");
        self.get(&path).map_or(0, |body| body.lines().count())
    }
    fn stop(mut self) -> String {
        assert!(self.0.try_wait().unwrap().is_none(), "serve exited");
        let _ = self.0.kill();
        let mut text = String::new();
        let _ = self.0.stderr.take().unwrap().read_to_string(&mut text);
        text
    }
}
impl Drop for Serve {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}
fn wait(ready: impl Fn() -> bool) {
    let start = Instant::now();
    while !ready() {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn probe(serve: &Serve, spool: &Path, sealed: u32) -> usize {
    let endpoint = format!("http://127.0.0.1:{}", serve.1);
    let spec = locus::reporter::Spec::api(endpoint, spool);
    let policy = Policy::default().producer("probe").reporter(spec);
    let engine = Engine::bootstrap(Config::new(policy)).unwrap();
    for sequence in 0..5 {
        let candidate = Candidate::event(json!({"sequence": sequence}));
        engine
            .append(&Context::empty(), candidate.ensure(Role::trace()))
            .unwrap();
    }
    let segment = spool.join(format!("sealed-{sealed:020}.jsonl"));
    std::fs::rename(spool.join("active.jsonl"), segment).unwrap();
    let expected = 5 * sealed as usize;
    wait(|| serve.stored("probe") == expected);
    expected
}
fn records(directory: &Path) -> Vec<Value> {
    let mut atoms = Vec::new();
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy();
            if name == "active.jsonl" || name.starts_with("sealed-") {
                let text = std::fs::read_to_string(path).unwrap();
                atoms.extend(
                    text.lines()
                        .filter_map(|line| serde_json::from_str(line).ok()),
                );
            }
        }
    }
    atoms
}
impl Drop for Nest {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
const ENABLED: &str = "[locus]\nenabled = true\nendpoint = 'http://127.0.0.1:0'\n";
fn spans(atoms: &[Value], function: &str, edge: &str) -> Vec<String> {
    let found = |a: &&Value| a["source"]["function"] == function && a["source"]["edge"] == edge;
    let atoms = atoms.iter().filter(found);
    atoms
        .map(|a| a["context"]["locus.span"].to_string())
        .collect()
}
#[test]
fn watched() {
    let n = Nest::new();
    std::fs::create_dir_all(n.seat().join("store")).unwrap();
    let port = port();
    n.configure(&format!(
        "[locus]\nenabled = true\nendpoint = 'http://127.0.0.1:{port}'\n"
    ));
    let serve = n.serve(port);
    let buffer = n.seat().join("state/locus-api");
    let probed = probe(&serve, &n.0.join("probe"), 1);
    let counted = |a: &Value| a["payload"]["event"] == "api.drain" && a["payload"]["records"] == 5;
    wait(|| records(&buffer).iter().any(counted));
    assert_eq!(serve.stored("probe"), probed);
    assert_eq!(serve.stored("locus"), 0);
    let stderr = serve.stop();
    assert!(!stderr.contains("registration"), "{stderr}");
    let atoms = records(&buffer);
    assert!(atoms.iter().all(|a| a["producer"] == "locus"));
    assert!(
        atoms
            .iter()
            .all(|a| a["context"]["locus.command"] == "api.serve")
    );
    for (function, event) in [
        ("drain", "api.drain"),
        ("seal", "api.seal"),
        ("retain", "api.retention"),
        ("migrate", "api.migrate"),
    ] {
        let entered = spans(&atoms, function, "enter");
        let returned = spans(&atoms, function, "return");
        assert!(!returned.is_empty() && returned.iter().all(|span| entered.contains(span)));
        let facts = atoms.iter().filter(|a| a["payload"]["event"] == event);
        assert!(
            facts
                .map(|a| a["context"]["locus.span"].to_string())
                .all(|s| entered.contains(&s))
        );
    }
    assert_eq!(spans(&atoms, "migrate", "enter").len(), 1);
    let migrated = atoms
        .iter()
        .filter(|a| a["payload"]["event"] == "api.migrate");
    assert_eq!(
        migrated
            .map(|a| a["payload"]["days"].clone())
            .collect::<Vec<_>>(),
        [json!(0)]
    );
    let retained = atoms
        .iter()
        .find(|a| a["payload"]["event"] == "api.retention")
        .unwrap();
    assert_eq!(retained["payload"]["outcome"], "ok");
    assert!(!buffer.join("registration").exists());
    let spools = std::fs::read_to_string(n.seat().join("spools.json")).unwrap();
    assert!(spools.contains("probe") && !spools.contains("state"));
    let text = serde_json::to_string(&atoms).unwrap();
    assert!(!text.contains(n.0.to_str().unwrap()));
}
#[test]
fn shielded() {
    let broken = Nest::new();
    broken.configure("[locus]\nenabled = 'private-setting'\n");
    let blocked = Nest::new();
    blocked.configure(ENABLED);
    std::fs::write(blocked.seat().join("state"), "occupied").unwrap();
    for n in [&broken, &blocked] {
        let serve = n.serve(port());
        assert_eq!(probe(&serve, &n.0.join("probe"), 1), 5);
        let stderr = serve.stop();
        assert!(stderr.contains("locus observation") && !stderr.contains("private-setting"));
    }
    let failing = Nest::new();
    failing.configure(ENABLED);
    let serve = failing.serve(port());
    let buffer = failing.seat().join("state/locus-api");
    probe(&serve, &failing.0.join("probe"), 1);
    std::fs::remove_dir_all(&buffer).unwrap();
    std::fs::write(&buffer, "occupied").unwrap();
    assert_eq!(probe(&serve, &failing.0.join("probe"), 2), 10);
    serve.stop();
}
#[test]
fn unregistered() {
    let n = Nest::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    n.configure(&format!(
        "[locus]\nenabled = true\nendpoint = 'http://127.0.0.1:{port}'\n"
    ));
    let listen = format!("127.0.0.1:{port}");
    let seat = n.seat().display().to_string();
    let out = n.run(&["serve", "--listen", &listen, "--home", &seat]);
    assert_eq!(out.status.code(), Some(2));
    assert!(n.seat().join("state/locus-api/spool.lock").exists());
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err());
}
