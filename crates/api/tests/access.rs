use locus_api::access::{self, Readers};
use locus_api::column::Columns;
use locus_api::registry::Registry;
use locus_api::retention::Retention;
use locus_api::server::{self, Shared};
use locus_api::store::{Segments, Store};
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
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
            "locus-api-access-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
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

fn declared(home: &Path) -> Readers {
    let text = format!(
        "[retention]\ndays = 3\n\n[[reader]]\nname = \"liberte\"\ndigest = \"{}\"\n\n[[reader]]\nname = \"locus.santi\"\ndigest = \"{}\"\n",
        access::digest("alpha"),
        access::digest("beta").to_ascii_uppercase()
    );
    fs::write(home.join("locus-api.toml"), text).expect("config");
    Readers::read(home).expect("readers")
}

fn online(home: &Path, readers: Readers) -> SocketAddr {
    let store: Arc<dyn Store> = Arc::new(Segments::open(home.join("store")).expect("store"));
    let registry = Arc::new(Registry::open(home.join("spools.json")).expect("registry"));
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
            let shared = Shared::new(store, registry, Retention::default()).readers(readers);
            server::serve(listener, shared, std::future::pending())
                .await
                .expect("serve");
        });
    });
    receiver.recv().expect("address")
}

fn call(address: SocketAddr, line: &str, authorization: Option<&str>, body: &str) -> String {
    let mut stream = TcpStream::connect(address).expect("connect");
    let auth = authorization.map_or(String::new(), |value| format!("authorization: {value}\r\n"));
    let request = format!(
        "{line} HTTP/1.1\r\nhost: locus\r\n{auth}content-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).expect("request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("response");
    response
}

fn status(response: &str) -> &str {
    response.split(' ').nth(1).expect("status")
}

#[test]
fn open() {
    let home = Temp::new("open");
    let address = online(home.path(), Readers::default());
    assert_eq!(status(&call(address, "GET /api/v1/atoms", None, "")), "200");
}

#[test]
fn guarded() {
    let home = Temp::new("guarded");
    let address = online(home.path(), declared(home.path()));
    for authorization in [
        None,
        Some("Bearer wrong"),
        Some("Basic YWxwaGE="),
        Some("Bearer "),
    ] {
        let response = call(address, "GET /api/v1/atoms", authorization, "");
        assert_eq!(status(&response), "401", "{authorization:?}");
        assert!(
            response
                .to_ascii_lowercase()
                .contains("www-authenticate: bearer")
        );
        assert!(response.contains("request.unauthorized"));
    }
    for token in ["alpha", "beta"] {
        let bearer = format!("Bearer {token}");
        let response = call(address, "GET /api/v1/atoms", Some(&bearer), "");
        assert_eq!(status(&response), "200", "{token}");
    }
    assert_eq!(status(&call(address, "GET /health", None, "")), "200");
    let enrollment = r#"{"producer":"concord","path":"/nonexistent"}"#;
    let spools = call(address, "POST /api/v1/spools", None, enrollment);
    assert_ne!(status(&spools), "401");
}

#[test]
fn refused() {
    let digest = access::digest("alpha");
    let cases = [
        format!(
            "[[reader]]\nname = \"a\"\ndigest = \"{digest}\"\n[[reader]]\nname = \"a\"\ndigest = \"{}\"\n",
            access::digest("b")
        ),
        format!(
            "[[reader]]\nname = \"a\"\ndigest = \"{digest}\"\n[[reader]]\nname = \"b\"\ndigest = \"{digest}\"\n"
        ),
        "[[reader]]\nname = \"a\"\ndigest = \"abc\"\n".to_string(),
        "[[reader]]\nname = \"a b\"\ndigest = \"{digest}\"\n".replace("{digest}", &digest),
        format!("[[reader]]\nname = \"a\"\ndigest = \"{digest}\"\ntoken = \"alpha\"\n"),
    ];
    for text in cases {
        let home = Temp::new("refused");
        fs::write(home.path().join("locus-api.toml"), &text).expect("config");
        assert!(Readers::read(home.path()).is_err(), "{text}");
        let report = doctor(home.path(), "127.0.0.1:9");
        assert_eq!(report["state"], "failing", "{text}");
    }
}

#[test]
fn retention() {
    let home = Temp::new("retention");
    let readers = declared(home.path());
    assert_eq!(readers.names(), ["liberte", "locus.santi"]);
    assert_eq!(
        Retention::read(home.path()).expect("retention").declared(),
        Some(3)
    );
}

#[test]
fn doctored() {
    let home = Temp::new("doctored");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    drop(
        runtime
            .block_on(Columns::open(home.path(), runtime.handle().clone()))
            .expect("columns"),
    );
    drop(runtime);
    let spool = home.path().join("spool");
    fs::create_dir_all(&spool).expect("spool");
    fs::write(spool.join("spool.lock"), b"").expect("lock");
    let address = online(home.path(), declared(home.path()));
    let report = doctor(home.path(), &address.to_string());
    assert_eq!(report["state"], "ready", "{report}");
    let atoms = report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|check| check["name"] == "atoms")
        .expect("atoms");
    assert!(atoms["detail"].as_str().expect("detail").contains("401"));
}

fn doctor(home: &Path, listen: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_locus-api"))
        .args(["doctor", "--home"])
        .arg(home)
        .args(["--listen", listen])
        .output()
        .expect("doctor");
    serde_json::from_slice(&output.stdout).expect("json report")
}
