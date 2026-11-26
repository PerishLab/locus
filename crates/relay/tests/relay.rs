use axum::Router;
use flate2::Compression;
use flate2::write::GzEncoder;
use locus::Atom;
use locus_relay::{Config, Relay, router};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener as Listener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;
use tokio::net::TcpListener;

struct Capture {
    line: String,
    headers: HashMap<String, String>,
    body: String,
}

fn upstream() -> (SocketAddr, Receiver<Capture>) {
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("addr");
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        while let Ok((stream, _)) = listener.accept() {
            if let Some(item) = read(stream) {
                let _ = sender.send(item);
            }
        }
    });
    (address, receiver)
}

fn read(mut stream: TcpStream) -> Option<Capture> {
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
    let _ = stream
        .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    Some(Capture {
        line,
        headers,
        body: String::from_utf8(body).ok()?,
    })
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).expect("write");
    encoder.finish().expect("finish")
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .new_agent()
}

fn atom(id: &str, at: u64, payload: &str) -> String {
    format!(
        r#"{{"at":{at},"id":"{id}","producer":"folio","context":{{"locus.trace":"t1"}},"choices":[],"payload":{payload}}}"#
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn endtoend() {
    let (mock, receiver) = upstream();
    let root = tempfile::tempdir().expect("tempdir");
    let config = Config::new(
        format!("http://{mock}"),
        "machine-secret",
        "folio",
        root.path().join("buffer"),
    )
    .origins(vec!["https://folio.perish.uk".into()]);
    let relay = Relay::open(config).expect("open");
    let app = Router::new().nest("/api/observe", router(relay.clone()));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listen");
    let address = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .expect("serve");
    });

    let plain = atom(
        "e1",
        1_700_000_000_000_000_001,
        r#"{"compass":"north","bypass":false}"#,
    );
    let second = atom("e2", 1_700_000_000_000_000_002, r#"{"passage":1}"#);
    let zipped = gzip(format!("{second}\n").as_bytes());
    let url = format!("http://{address}/api/observe");
    let plaincopy = plain.clone();
    tokio::task::spawn_blocking(move || {
        let client = agent();
        let first = client
            .post(&url)
            .header("origin", "https://folio.perish.uk")
            .header("sec-fetch-site", "same-origin")
            .content_type("application/x-ndjson")
            .send(format!("{plaincopy}\n").as_bytes())
            .expect("plain");
        assert_eq!(first.status().as_u16(), 204);

        let compressed = client
            .post(&url)
            .header("origin", "https://folio.perish.uk")
            .header("sec-fetch-site", "Same-Origin")
            .header("content-encoding", "gzip")
            .content_type("application/x-ndjson")
            .send(zipped.as_slice())
            .expect("gzip");
        assert_eq!(compressed.status().as_u16(), 204);
    })
    .await
    .expect("join");

    relay.settle().expect("settle");
    let mut received = String::new();
    let mut keys = Vec::new();
    while !received.contains(r#""id":"e1""#) || !received.contains(r#""id":"e2""#) {
        let item = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("upstream");
        assert_eq!(item.line, "POST /api/v1/atoms?producer=folio HTTP/1.1");
        assert_eq!(
            item.headers.get("authorization").map(String::as_str),
            Some("Bearer machine-secret")
        );
        let key = item
            .headers
            .get("idempotency-key")
            .expect("idempotency-key")
            .clone();
        assert_eq!(key.len(), 64);
        keys.push(key);
        received.push_str(&item.body);
    }
    for line in received.lines() {
        let parsed: Atom = serde_json::from_str(line).expect("atom");
        assert_eq!(parsed.producer(), Some("folio"));
    }
    assert!(received.contains(&plain));
    assert!(received.contains(&second));
    assert!(!keys.is_empty());
}
