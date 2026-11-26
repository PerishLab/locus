use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Extensions, HeaderMap, HeaderName, HeaderValue};
use flate2::Compression;
use flate2::write::GzEncoder;
use locus_relay::{Config, Relay, ingest, router};
use std::io::Write;
use tokio::net::TcpListener;

struct Harness {
    _root: tempfile::TempDir,
    relay: Relay,
    url: String,
}

impl Harness {
    async fn start(origins: Vec<String>, burst: u32, forwarded: bool) -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let config = Config::new(
            "http://127.0.0.1:9",
            "writer-key",
            "folio",
            root.path().join("buffer"),
        )
        .origins(origins)
        .rpm(60)
        .burst(burst)
        .forwarded(forwarded);
        let debug = format!("{config:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("writer-key"));
        let relay = Relay::open(config).expect("open");
        let app = Router::new().nest("/api/observe", router(relay.clone()));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listen");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        Self {
            _root: root,
            relay,
            url: format!("http://{address}/api/observe"),
        }
    }
}

async fn direct(relay: &Relay, headers: &[(&str, &str)], body: Vec<u8>) -> u16 {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        map.append(
            HeaderName::from_bytes(name.as_bytes()).expect("header"),
            HeaderValue::from_str(value).expect("value"),
        );
    }
    ingest(
        State(relay.clone()),
        map,
        Extensions::new(),
        Body::from(body),
    )
    .await
    .status()
    .as_u16()
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .new_agent()
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).expect("write");
    encoder.finish().expect("finish")
}

fn valid(id: &str) -> String {
    format!(
        r#"{{"at":1700000000000000001,"id":"{id}","producer":"folio","context":{{"locus.trace":"t1"}},"choices":[]}}"#
    )
}

fn post(url: &str, headers: &[(&str, &str)], body: &[u8]) -> (u16, Option<String>) {
    let client = agent();
    let mut req = client.post(url).content_type("application/x-ndjson");
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let reply = req.send(body).expect("send");
    let retry = reply
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    (reply.status().as_u16(), retry)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn origins() {
    let harness = Harness::start(vec!["https://FOLIO.PERISH.UK:443".into()], 30, false).await;
    let url = harness.url.clone();
    let body = format!("{}\n", valid("o1"));
    assert_eq!(
        direct(
            &harness.relay,
            &[("origin", "https://evil.example")],
            vec![b'x'; 512 * 1024]
        )
        .await,
        403
    );
    tokio::task::spawn_blocking(move || {
        assert_eq!(post(&url, &[], body.as_bytes()).0, 403);
        assert_eq!(
            post(
                &url,
                &[
                    ("origin", "https://folio.perish.uk"),
                    ("sec-fetch-site", "cross-site")
                ],
                body.as_bytes()
            )
            .0,
            403
        );
        assert_eq!(
            post(
                &url,
                &[
                    ("origin", "https://folio.perish.uk"),
                    ("sec-fetch-site", "same-origin")
                ],
                body.as_bytes()
            )
            .0,
            204
        );
    })
    .await
    .expect("join");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn throttle() {
    let direct = Harness::start(Vec::new(), 2, false).await;
    let trusted = Harness::start(Vec::new(), 2, true).await;
    let url1 = direct.url.clone();
    let url2 = trusted.url.clone();
    let body = format!("{}\n", valid("r1"));
    tokio::task::spawn_blocking(move || {
        assert_eq!(
            post(&url1, &[("x-forwarded-for", "1.1.1.1")], body.as_bytes()).0,
            204
        );
        assert_eq!(
            post(&url1, &[("x-forwarded-for", "2.2.2.2")], body.as_bytes()).0,
            204
        );
        let (code, retry) = post(&url1, &[("x-real-ip", "3.3.3.3")], body.as_bytes());
        assert_eq!(code, 429);
        assert!(retry.expect("retry").parse::<u64>().expect("u64") >= 1);

        assert_eq!(
            post(
                &url2,
                &[("x-forwarded-for", "1.1.1.1, 203.0.113.10")],
                body.as_bytes()
            )
            .0,
            204
        );
        assert_eq!(
            post(
                &url2,
                &[("x-forwarded-for", "9.9.9.9, 203.0.113.10")],
                body.as_bytes()
            )
            .0,
            204
        );
        let (code, retry) = post(
            &url2,
            &[("x-forwarded-for", "8.8.8.8, 203.0.113.10")],
            body.as_bytes(),
        );
        assert_eq!(code, 429);
        assert!(retry.expect("retry").parse::<u64>().expect("u64") >= 1);
        assert_eq!(
            post(&url2, &[("x-real-ip", "203.0.113.11")], body.as_bytes()).0,
            204
        );
    })
    .await
    .expect("join");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encoding() {
    let harness = Harness::start(Vec::new(), 30, false).await;
    let url = harness.url.clone();
    let line = format!("{}\n", valid("c1"));
    let bomb = gzip(&vec![b'a'; 256 * 1024 + 1]);
    let mut trailing = gzip(line.as_bytes());
    trailing.extend_from_slice(b"junk");
    assert_eq!(
        direct(
            &harness.relay,
            &[("content-encoding", "gzip")],
            vec![0u8; 128 * 1024 + 1]
        )
        .await,
        413
    );
    assert_eq!(
        direct(&harness.relay, &[], vec![b'x'; 256 * 1024 + 1]).await,
        413
    );
    tokio::task::spawn_blocking(move || {
        assert_eq!(
            post(&url, &[("content-encoding", "identity")], line.as_bytes()).0,
            204
        );
        assert_eq!(
            post(&url, &[("content-encoding", "gzip")], b"corrupt").0,
            400
        );
        assert_eq!(
            post(&url, &[("content-encoding", "gzip")], &trailing).0,
            400
        );
        assert_eq!(post(&url, &[("content-encoding", "gzip")], &bomb).0, 413);
        assert_eq!(
            post(&url, &[("content-encoding", "br")], line.as_bytes()).0,
            415
        );
    })
    .await
    .expect("join");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn payload() {
    let harness = Harness::start(Vec::new(), 30, false).await;
    let url = harness.url.clone();
    let many: String = (0..513)
        .map(|i| format!("{}\n", valid(&format!("m{i}"))))
        .collect();
    let benign = br#"{"at":1,"id":"ok","producer":"folio","context":{"metric.name":"lat","metric.unit":"ms","metric.kind":"gauge","metric.series":"s1","metric.label.workload":"web","metric.epoch":"e1","trace":"t1","span":"s1","parent":"p1"},"choices":[{"role":"trace","key":"t1","origin":"explicit"}],"payload":{"level":"info","message":"ok","elapsed":12,"status":200,"error":false,"reason":"none","code":"OK"}}"#;
    let cases = [
        b"".as_slice(),
        b"\n",
        &[0xff, 0xfe],
        br#"{"bad":true}"#,
        br#"{"at":1,"id":"x","producer":"wrong","context":{},"choices":[]}"#,
        br#"{"at":1,"id":"","producer":"folio","context":{},"choices":[]}"#,
        br#"{"at":1,"id":"x","producer":"folio","context":{},"choices":[],"password":"leaked"}"#,
        br#"{"at":1,"id":"x","producer":"folio","context":{},"choices":[],"extra":1}"#,
        br#"{"at":1,"id":"x","producer":"folio","context":{},"choices":[{"role":"auth.token","key":"v","origin":"explicit"}]}"#,
        br#"{"at":1,"id":"x","producer":"folio","context":{},"choices":[{"role":"trace","key":"user_password","origin":"explicit"}]}"#,
        br#"{"at":1,"id":"x","producer":"folio","context":{},"choices":[],"collections":[{"role":"session_cookie","binding":"b","collector":"c","selector":"s"}]}"#,
        br#"{"at":1,"id":"x","producer":"folio","context":{"auth.token":"1"},"choices":[]}"#,
        br#"{"at":1,"id":"x","producer":"folio","context":{},"choices":[],"payload":{"nested":{"userPassword":"s"}}}"#,
        br#"{"at":1,"id":"x","producer":"folio","context":{},"choices":[],"payload":[{"code_verifier":"v"}]}"#,
        br#"{"at":1,"id":"x","producer":"folio","context":{},"choices":[],"payload":{"header/Cookie":"c"}}"#,
        br#"{"at":1,"id":"x","producer":"folio","context":{},"choices":[],"payload":{"rescueCode":"r"}}"#,
        br#"{"at":1,"id":"x","producer":"folio","context":{},"choices":[],"payload":{"sudoMode":true}}"#,
    ];
    tokio::task::spawn_blocking(move || {
        assert_eq!(post(&url, &[], benign).0, 204);
        for case in cases {
            assert_eq!(post(&url, &[], case).0, 400);
        }
        assert_eq!(post(&url, &[], many.as_bytes()).0, 413);
    })
    .await
    .expect("join");
}
