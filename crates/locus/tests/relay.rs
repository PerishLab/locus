use locus::Atom;
use locus::relay::{Config, Relay};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

struct Request {
    line: String,
    headers: HashMap<String, String>,
    body: String,
}

fn server() -> (SocketAddr, Receiver<Request>) {
    serve(0)
}

fn serve(reject: usize) -> (SocketAddr, Receiver<Request>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("addr");
    let (sender, receiver) = mpsc::channel();
    let seen = AtomicUsize::new(0);
    thread::spawn(move || {
        while let Ok((stream, _)) = listener.accept() {
            let status = if seen.fetch_add(1, Ordering::Relaxed) < reject {
                "503 Service Unavailable"
            } else {
                "204 No Content"
            };
            if let Some(request) = handle(stream, status) {
                let _ = sender.send(request);
            }
        }
    });
    (address, receiver)
}

fn handle(mut stream: TcpStream, status: &str) -> Option<Request> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let split = loop {
        let count = stream.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..count]);
        if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
    };
    let head = String::from_utf8(buffer[..split].to_vec()).ok()?;
    let mut lines = head.split("\r\n");
    let line = lines.next()?.to_owned();
    let mut headers = HashMap::new();
    for item in lines {
        if let Some((name, value)) = item.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = buffer[split + 4..].to_vec();
    while body.len() < length {
        let count = stream.read(&mut chunk).ok()?;
        if count == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..count]);
    }
    body.truncate(length);
    let reply = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(reply.as_bytes());
    Some(Request {
        line,
        headers,
        body: String::from_utf8(body).ok()?,
    })
}

fn atom(id: &str, at: u64) -> String {
    format!(
        r#"{{"at":{at},"id":"{id}","producer":"folio","context":{{"locus.trace":"t1"}},"choices":[]}}"#
    )
}

#[test]
fn delivers() {
    let (address, receiver) = server();
    let root = tempfile::tempdir().expect("tempdir");
    let buffer = root.path().join("queue");
    let config = Config::new(
        format!("http://{address}"),
        "writer-secret",
        "folio",
        &buffer,
    );
    let formatted = format!("{config:?}");
    assert!(formatted.contains("[redacted]"));
    assert!(!formatted.contains("writer-secret"));
    let relay = Relay::open(config).expect("open");
    let first = atom("a1", 1_700_000_000_000_000_001);
    let second = atom("a2", 1_700_000_000_000_000_002);
    relay.enqueue(&[&first, &second]).expect("enqueue");
    relay.settle().expect("settle");
    let request = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("delivered");
    assert_eq!(request.line, "POST /api/v1/atoms?producer=folio HTTP/1.1");
    assert_eq!(
        request.headers.get("authorization").map(String::as_str),
        Some("Bearer writer-secret")
    );
    assert_eq!(
        request.headers.get("content-type").map(String::as_str),
        Some("application/x-ndjson")
    );
    let key = request
        .headers
        .get("idempotency-key")
        .expect("idempotency-key");
    assert_eq!(key.len(), 64);
    assert_eq!(request.body, format!("{first}\n{second}\n"));
    for line in request.body.lines() {
        let parsed: Atom = serde_json::from_str(line).expect("atom");
        assert_eq!(parsed.producer(), Some("folio"));
    }
}

#[test]
fn background() {
    let (address, receiver) = server();
    let root = tempfile::tempdir().expect("tempdir");
    let buffer = root.path().join("queue");
    let config = Config::new(format!("http://{address}"), "key-bg", "folio", &buffer);
    let relay = Relay::open(config).expect("open");
    let line = atom("bg1", 1_700_000_000_000_000_010);
    relay.push(&line).expect("push");
    let request = receiver.recv_timeout(Duration::from_secs(5)).expect("wake");
    assert_eq!(request.body, format!("{line}\n"));
}

#[test]
fn recovery() {
    let (address, receiver) = serve(1);
    let root = tempfile::tempdir().expect("tempdir");
    let buffer = root.path().join("queue");
    let config = Config::new(format!("http://{address}"), "key-retry", "folio", &buffer);
    let relay = Relay::open(config).expect("open");
    let line = atom("r1", 1_700_000_000_000_000_020);
    relay.push(&line).expect("push");
    let first = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("attempt");
    let second = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("retry");
    assert_eq!(first.body, format!("{line}\n"));
    assert_eq!(second.body, first.body);
    assert_eq!(
        second.headers.get("idempotency-key"),
        first.headers.get("idempotency-key")
    );
}

#[test]
fn startup() {
    let probe = TcpListener::bind("127.0.0.1:0").expect("probe");
    let address = probe.local_addr().expect("addr");
    drop(probe);
    let root = tempfile::tempdir().expect("tempdir");
    let buffer = root.path().join("queue");
    let line = atom("s1", 1_700_000_000_000_000_030);
    let first = Relay::open(Config::new(
        format!("http://{address}"),
        "key-boot",
        "folio",
        &buffer,
    ))
    .expect("open");
    first.push(&line).expect("push");
    drop(first);

    let listener = TcpListener::bind(address).expect("rebind");
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept()
            && let Some(request) = handle(stream, "204 No Content")
        {
            let _ = sender.send(request);
        }
    });
    let _second = Relay::open(Config::new(
        format!("http://{address}"),
        "key-boot",
        "folio",
        &buffer,
    ))
    .expect("reopen");
    let request = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("drained");
    assert_eq!(request.body, format!("{line}\n"));
}

#[test]
fn guards() {
    let root = tempfile::tempdir().expect("tempdir");
    let buffer = root.path().join("queue");
    for bad in [
        "http://example.com",
        "http://localhost:8080",
        "https://example.com/path",
    ] {
        assert!(Relay::open(Config::new(bad, "k", "folio", &buffer)).is_err());
    }
    assert!(Relay::open(Config::new("https://example.com", "", "folio", &buffer)).is_err());
    assert!(
        Relay::open(Config::new(
            "https://example.com",
            "k",
            "folio",
            Path::new("relative")
        ))
        .is_err()
    );
    let relay =
        Relay::open(Config::new("http://127.0.0.1:9", "k", "folio", &buffer)).expect("open");
    assert!(relay.push("").is_err());
    assert!(relay.push("a\nb").is_err());
    assert!(relay.push("a\rb").is_err());
    let huge = "x".repeat(1024 * 1024);
    assert!(relay.push(&huge).is_err());
    assert!(Relay::open(Config::new("http://127.0.0.1:9", "k", "other", &buffer)).is_err());
    assert!(locus::reporter::spool::claim(&buffer).is_err());
}
