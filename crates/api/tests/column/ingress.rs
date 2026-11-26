use axum::routing::post;
use axum::{Json, Router};
use flate2::Compression;
use flate2::write::GzEncoder;
use locus_api::column::{Binding, Columns, Mode, assign};
use locus_api::retention::Retention;
use locus_api::server::{Authority, edge};
use locus_api::store::Producer;
use radicula_oidc::exchange;
use radicula_oidc::jwt::{Algorithm, Audience, Claims, EncodingKey, Signer};
use serde_json::{Map, json};
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;

const PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgFE5NqbEKTODbefQY\n\
aKzRlamCulP7ORDxZsZTygd0eeWhRANCAATh+MggYmFJgYwVlzU7zcqusAP4br+d\n\
LJiO5JW2eEeu04iGN+9rLAWTLSfFcphkeGkA60NxGBKLs3q9EALyTbHI\n\
-----END PRIVATE KEY-----\n";
const X: &str = "4fjIIGJhSYGMFZc1O83KrrAD-G6_nSyYjuSVtnhHrtM";
const Y: &str = "iIY372ssBZMtJ8VymGR4aQDrQ3EYEouzer0QAvJNscg";

struct Harness {
    _home: tempfile::TempDir,
    address: SocketAddr,
}

impl Harness {
    async fn start() -> Self {
        let issuer = ensign().await;
        let home = tempfile::tempdir().expect("tempdir");
        let jwks = json!({
            "keys": [{
                "kty": "EC",
                "kid": "k1",
                "alg": "ES256",
                "crv": "P-256",
                "use": "sig",
                "x": X,
                "y": Y
            }]
        });
        fs::write(home.path().join("keys.json"), jwks.to_string()).expect("jwks");
        let config = format!(
            "[ensign]\nlisten = \"127.0.0.1:0\"\nissuer = \"{issuer}\"\nkeys = \"keys.json\"\n"
        );
        fs::write(home.path().join("locus-api.toml"), config).expect("config");
        let columns = Arc::new(
            Columns::open(home.path(), tokio::runtime::Handle::current())
                .await
                .expect("columns"),
        );
        let producer = Producer::new("folio").expect("producer");
        for mode in [Mode::Append, Mode::Scan] {
            let binding = Binding {
                issuer: issuer.clone(),
                subject: "writer".into(),
                grant: true,
            };
            assign(home.path(), &binding, &producer, mode)
                .await
                .expect("grant");
        }
        let (_, authority) = Authority::load(home.path())
            .expect("authority")
            .expect("configured");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listen");
        let address = listener.local_addr().expect("addr");
        let app = edge(columns, authority, Retention::default());
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });
        Self {
            _home: home,
            address,
        }
    }

    fn url(&self) -> String {
        format!("http://{}/api/v1/atoms?producer=folio", self.address)
    }
}

async fn ensign() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("issuer");
    let address = listener.local_addr().expect("addr");
    let issuer = format!("http://{address}");
    let url = issuer.clone();
    let app = Router::new().route(
        "/token",
        post(move || {
            let issuer = url.clone();
            async move {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock")
                    .as_secs();
                let signer = Signer {
                    algorithm: Algorithm::ES256,
                    kid: "k1".into(),
                    key: EncodingKey::from_ec_pem(PEM.as_bytes()).expect("pem"),
                };
                let claims = Claims {
                    iss: issuer,
                    sub: "writer".into(),
                    aud: Audience::One("locus".into()),
                    exp: now + 300,
                    iat: now,
                    nbf: None,
                    extra: Map::from_iter([("kind".into(), json!("access"))]),
                };
                let signed = signer.sign(&claims).expect("sign");
                Json(json!({
                    "access_token": signed.expose(),
                    "issued_token_type": exchange::ACCESS,
                    "token_type": "Bearer",
                    "expires_in": 300
                }))
            }
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("ensign");
    });
    issuer
}

fn client() -> ureq::Agent {
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

fn send(url: &str, key: &str, encoding: Option<&str>, body: &[u8]) -> u16 {
    let agent = client();
    let mut request = agent
        .post(url)
        .header("authorization", "Bearer device-key")
        .header("idempotency-key", key)
        .content_type("application/x-ndjson");
    if let Some(encoding) = encoding {
        request = request.header("content-encoding", encoding);
    }
    request.send(body).expect("send").status().as_u16()
}

fn read(url: &str) -> String {
    let mut reply = client()
        .get(url)
        .header("authorization", "Bearer device-key")
        .call()
        .expect("get");
    assert_eq!(reply.status().as_u16(), 200);
    reply.body_mut().read_to_string().expect("body")
}

fn sample(id: &str, at: u64) -> String {
    format!(
        r#"{{"at":{at},"id":"{id}","producer":"folio","context":{{"locus.trace":"t1"}},"choices":[]}}"#
    )
}

fn duplicate(address: SocketAddr, body: &[u8]) -> String {
    let mut stream = TcpStream::connect(address).expect("connect");
    let head = format!(
        "POST /api/v1/atoms?producer=folio HTTP/1.1\r\nhost: locus\r\nauthorization: Bearer device-key\r\nidempotency-key: k-dup\r\ncontent-type: application/x-ndjson\r\ncontent-encoding: gzip\r\ncontent-encoding: gzip\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).expect("head");
    stream.write_all(body).expect("body");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("response");
    response.split(' ').nth(1).expect("status").to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compressed() {
    let harness = Harness::start().await;
    let url = harness.url();
    let batch = format!(
        "{}\n{}\n",
        sample("g1", 1_700_000_000_000_000_001),
        sample("g2", 1_700_000_000_000_000_002)
    );
    let zipped = gzip(batch.as_bytes());
    let plain = format!("{}\n", sample("p1", 1_700_000_000_000_000_003));
    let identity = format!("{}\n", sample("p2", 1_700_000_000_000_000_004));
    let url1 = url.clone();
    tokio::task::spawn_blocking(move || {
        assert_eq!(send(&url1, "k-gzip", Some(" GZip "), &zipped), 204);
        assert_eq!(send(&url1, "k-gzip", Some("gzip"), &zipped), 204);
        assert_eq!(send(&url1, "k-plain", None, plain.as_bytes()), 204);
        assert_eq!(
            send(&url1, "k-identity", Some("identity"), identity.as_bytes()),
            204
        );
        let stored = read(&url1);
        assert_eq!(stored.matches(r#""id":"g1""#).count(), 1);
        assert_eq!(stored.matches(r#""id":"g2""#).count(), 1);
        assert_eq!(stored.matches(r#""id":"p1""#).count(), 1);
        assert_eq!(stored.matches(r#""id":"p2""#).count(), 1);
    })
    .await
    .expect("join");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejected() {
    let harness = Harness::start().await;
    let url = harness.url();
    let address = harness.address;
    let bomb = gzip(&vec![b'a'; 1_048_576 + 1]);
    let oversized = vec![0u8; 256 * 1024 + 1];
    let plain = format!("{}\n", sample("u1", 1_700_000_000_000_000_010));
    let mut trailing = gzip(plain.as_bytes());
    trailing.extend_from_slice(b"junk");
    let valid = gzip(plain.as_bytes());
    tokio::task::spawn_blocking(move || {
        assert_eq!(send(&url, "k-corrupt", Some("gzip"), b"not-gzip"), 400);
        assert_eq!(send(&url, "k-trailing", Some("gzip"), &trailing), 400);
        assert_eq!(send(&url, "k-bomb", Some("gzip"), &bomb), 413);
        assert_eq!(send(&url, "k-huge", Some("gzip"), &oversized), 413);
        assert_eq!(send(&url, "k-br", Some("br"), plain.as_bytes()), 415);
        assert_eq!(duplicate(address, &valid), "415");
    })
    .await
    .expect("join");
}
