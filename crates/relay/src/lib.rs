mod check;
mod codec;
mod limit;
mod origin;

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{Extensions, HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use limit::Limiter;
use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error(String);

impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<locus::Error> for Error {
    fn from(error: locus::Error) -> Self {
        Self(error.to_string())
    }
}

#[derive(Clone)]
pub struct Config {
    pub endpoint: String,
    pub key: String,
    pub producer: String,
    pub buffer: PathBuf,
    pub origins: Vec<String>,
    pub rpm: u32,
    pub burst: u32,
    pub forwarded: bool,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("endpoint", &self.endpoint)
            .field("key", &"[redacted]")
            .field("producer", &self.producer)
            .field("buffer", &self.buffer)
            .field("origins", &self.origins)
            .field("rpm", &self.rpm)
            .field("burst", &self.burst)
            .field("forwarded", &self.forwarded)
            .finish()
    }
}

impl Config {
    pub fn new(
        endpoint: impl Into<String>,
        key: impl Into<String>,
        producer: impl Into<String>,
        buffer: impl Into<PathBuf>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            key: key.into(),
            producer: producer.into(),
            buffer: buffer.into(),
            origins: Vec::new(),
            rpm: 120,
            burst: 30,
            forwarded: false,
        }
    }

    pub fn origins(mut self, origins: Vec<String>) -> Self {
        self.origins = origins;
        self
    }

    pub fn rpm(mut self, rpm: u32) -> Self {
        self.rpm = rpm;
        self
    }

    pub fn burst(mut self, burst: u32) -> Self {
        self.burst = burst;
        self
    }

    pub fn forwarded(mut self, forwarded: bool) -> Self {
        self.forwarded = forwarded;
        self
    }
}

struct Inner {
    producer: String,
    origins: Vec<String>,
    forwarded: bool,
    limiter: Limiter,
    queue: Arc<locus::relay::Relay>,
}

#[derive(Clone)]
pub struct Relay(Arc<Inner>);

impl Relay {
    pub fn open(config: Config) -> Result<Self, Error> {
        if config.rpm == 0 || config.burst == 0 {
            return Err(Error::new("relay rpm and burst must be nonzero"));
        }
        let origins = origin::parse(config.origins)?;
        let producer = config.producer.clone();
        let queue = locus::relay::Relay::open(locus::relay::Config::new(
            config.endpoint,
            config.key,
            config.producer,
            config.buffer,
        ))?;
        Ok(Self(Arc::new(Inner {
            producer,
            origins,
            forwarded: config.forwarded,
            limiter: Limiter::new(config.rpm, config.burst),
            queue: Arc::new(queue),
        })))
    }

    pub fn settle(&self) -> Result<(), String> {
        self.0.queue.settle()
    }
}

pub fn router(relay: Relay) -> Router {
    Router::new().route("/", post(ingest)).with_state(relay)
}

pub async fn ingest(
    State(relay): State<Relay>,
    headers: HeaderMap,
    extensions: Extensions,
    body: Body,
) -> Response {
    if let Err(status) = origin::check(&headers, &relay.0.origins) {
        return status.into_response();
    }
    let peer = extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let key = limit::client(&headers, peer, relay.0.forwarded);
    if let Err(wait) = relay.0.limiter.check(&key) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, wait.to_string())],
        )
            .into_response();
    }
    let raw = match codec::read(&headers, body).await {
        Ok(raw) => raw,
        Err(status) => return status.into_response(),
    };
    let lines = match check::batch(&raw, &relay.0.producer) {
        Ok(lines) => lines,
        Err(status) => return status.into_response(),
    };
    let queue = relay.0.queue.clone();
    match tokio::task::spawn_blocking(move || queue.enqueue(&lines)).await {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(_)) | Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
