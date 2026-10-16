mod producers;

use locus_api::column::Columns;
use locus_api::registry::Registry;
use locus_api::retention::Retention;
use locus_api::server::{self, Shared};
use locus_api::store::{Segments, Store};
use serde_json::Value;
use std::fs;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Temp(PathBuf);

impl Temp {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "locus-api-doctor-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn healthy(label: &str) -> Temp {
    let home = Temp::new(label);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let columns = runtime
        .block_on(Columns::open(home.path(), runtime.handle().clone()))
        .expect("columns");
    drop(columns);
    drop(runtime);
    let spool = home.path().join("spool");
    fs::create_dir_all(&spool).expect("spool");
    fs::write(spool.join("spool.lock"), b"").expect("lock");
    Registry::open(home.path().join("spools.json"))
        .expect("registry")
        .enroll("concord", &spool)
        .expect("enroll");
    home
}

fn offline() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().expect("address")
}

fn online() -> (SocketAddr, Temp) {
    let home = Temp::new("served");
    let store: Arc<dyn Store> = Arc::new(Segments::open(home.path().join("store")).expect("store"));
    let registry = Arc::new(Registry::open(home.path().join("spools.json")).expect("registry"));
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("listen");
            sender
                .send(listener.local_addr().expect("address"))
                .expect("send");
            let shared = Shared::new(store, registry, Retention::default());
            server::serve(listener, shared, std::future::pending())
                .await
                .expect("serve");
        });
    });
    (receiver.recv().expect("address"), home)
}

struct Report {
    code: Option<i32>,
    json: Value,
}

impl Report {
    fn check(&self, name: &str) -> &Value {
        self.json["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .find(|check| check["name"] == name)
            .unwrap_or_else(|| panic!("no {name} check in {}", self.json))
    }

    fn has(&self, name: &str) -> bool {
        self.json["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .any(|check| check["name"] == name)
    }

    fn failed(&self) -> bool {
        self.json["state"] == "failing"
    }
}

fn doctor(home: &Path, listen: SocketAddr) -> Report {
    let output = Command::new(env!("CARGO_BIN_EXE_locus-api"))
        .arg("doctor")
        .arg("--home")
        .arg(home)
        .arg("--listen")
        .arg(listen.to_string())
        .output()
        .expect("doctor");
    let json: Value = serde_json::from_slice(&output.stdout).expect("json report");
    let failed = json["state"] == "failing";
    assert_eq!(output.status.code(), Some(i32::from(failed)), "{json}");
    Report {
        code: output.status.code(),
        json,
    }
}

fn status(report: &Report, name: &str) -> String {
    report.check(name)["status"]
        .as_str()
        .expect("status")
        .to_string()
}

fn next(report: &Report, name: &str) -> String {
    report.check(name)["next"]
        .as_str()
        .unwrap_or_else(|| panic!("no next step for {name} in {}", report.json))
        .to_string()
}

fn detail(report: &Report, name: &str) -> String {
    report.check(name)["detail"]
        .as_str()
        .expect("detail")
        .to_string()
}

#[test]
fn ready() {
    let home = healthy("ready");
    let report = doctor(home.path(), offline());
    for name in ["binary", "estate", "column", "spools", "config"] {
        assert_eq!(status(&report, name), "ok", "{}", report.json);
    }
    assert_eq!(status(&report, "health"), "offline");
    assert!(!report.has("atoms"));
    assert!(!report.failed());
    assert_eq!(report.code, Some(0));
}

#[test]
fn loopback() {
    let home = healthy("loopback");
    let report = doctor(home.path(), "0.0.0.0:9".parse().expect("address"));
    assert_eq!(status(&report, "binary"), "fail");
    assert!(next(&report, "binary").contains("loopback"));
    assert!(report.failed());
}

#[test]
fn estate() {
    let home = healthy("estate");
    let path = home.path().join("estate.sqlite3");
    fs::write(&path, b"not a database at all").expect("corrupt");
    let corrupt = doctor(home.path(), offline());
    assert_eq!(status(&corrupt, "estate"), "fail");
    assert!(next(&corrupt, "estate").contains("backup"));
    fs::write(&path, b"SQLite").expect("truncate");
    let truncated = doctor(home.path(), offline());
    assert_eq!(status(&truncated, "estate"), "fail");
    assert!(detail(&truncated, "estate").contains("truncated"));
    fs::remove_file(&path).expect("remove");
    let missing = doctor(home.path(), offline());
    assert_eq!(status(&missing, "estate"), "fail");
    assert!(next(&missing, "estate").contains("serve"));
}

#[test]
fn column() {
    let home = healthy("column");
    let path = home.path().join("column");
    fs::remove_dir_all(&path).expect("remove");
    let missing = doctor(home.path(), offline());
    assert_eq!(status(&missing, "column"), "fail");
    assert!(detail(&missing, "column").contains("does not exist"));
    fs::write(&path, b"stray").expect("stray");
    let stray = doctor(home.path(), offline());
    assert_eq!(status(&stray, "column"), "fail");
    assert!(detail(&stray, "column").contains("not a directory"));
    fs::remove_file(&path).expect("remove stray");
    fs::create_dir(&path).expect("column");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions.clone()).expect("readonly");
    let readonly = doctor(home.path(), offline());
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).expect("writable");
    assert_eq!(status(&readonly, "column"), "fail");
    assert!(detail(&readonly, "column").contains("not writable"));
}

#[test]
fn spools() {
    let home = healthy("spools");
    let path = home.path().join("spools.json");
    fs::write(&path, b"{ not json").expect("corrupt");
    let corrupt = doctor(home.path(), offline());
    assert_eq!(status(&corrupt, "spools"), "fail");
    assert!(corrupt.failed());
    assert!(next(&corrupt, "spools").contains("re-register"));
    fs::remove_file(&path).expect("remove");
    let missing = doctor(home.path(), offline());
    assert_eq!(status(&missing, "spools"), "warn");
    assert!(next(&missing, "spools").contains("api reporter"));
    assert!(!missing.failed());
}

#[test]
fn config() {
    let home = healthy("config");
    let path = home.path().join("locus-api.toml");
    fs::write(&path, "[retention]\ndays = 7\n").expect("declared");
    let declared = doctor(home.path(), offline());
    assert_eq!(status(&declared, "config"), "ok");
    assert!(detail(&declared, "config").contains("7 day"));
    for malformed in [
        "[retention]\ndays = -1\n",
        "[retention]\nweeks = 1\n",
        "spool = 1\n",
        "[producers]\nnames = []\n",
    ] {
        fs::write(&path, malformed).expect("malformed");
        let report = doctor(home.path(), offline());
        assert_eq!(status(&report, "config"), "fail", "{malformed}");
        assert!(next(&report, "config").contains("[retention]"));
    }
}

#[test]
fn live() {
    let home = healthy("live");
    let (address, _served) = online();
    let report = doctor(home.path(), address);
    assert_eq!(status(&report, "health"), "ok", "{}", report.json);
    assert_eq!(status(&report, "atoms"), "ok", "{}", report.json);
    assert!(!report.failed());
    let spool = home.path().join("spool");
    let entries: Vec<_> = fs::read_dir(&spool).expect("spool").collect();
    assert_eq!(entries.len(), 1, "doctor never writes a spool");
}

#[test]
fn version() {
    let home = healthy("version");
    let report = doctor(home.path(), offline());
    assert!(
        report.json["version"]
            .as_str()
            .expect("version")
            .starts_with('v')
    );
    assert!(detail(&report, "binary").contains("is loopback"));
    fs::remove_file(home.path().join("estate.sqlite3")).expect("remove");
    let failing = doctor(home.path(), offline());
    assert_eq!(failing.code, Some(1));
    assert!(
        !home.path().join("estate.sqlite3").exists(),
        "doctor never bootstraps"
    );
}
