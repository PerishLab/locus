use crate::registry::Registry;
use crate::retention::{self, Retention};
use crate::store::{Filter, Producer, Store, Window};
use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use locus::{Key, Role};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::io;
use std::path::Path;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use utoipa::{IntoParams, IntoResponses, OpenApi, ToSchema};

const CHUNK: usize = 64 * 1024;
pub const RETAINED: &str = "locus-retained-from";

#[derive(Serialize, ToSchema)]
pub struct Health {
    status: &'static str,
}

#[derive(Serialize, ToSchema)]
pub struct Fault {
    fault: Detail,
}

#[derive(Serialize, ToSchema)]
pub struct Detail {
    code: &'static str,
    message: String,
}

#[derive(Deserialize, ToSchema)]
pub struct Enrollment {
    producer: String,
    path: String,
}

#[derive(Clone)]
pub struct Shared {
    store: Arc<dyn Store>,
    registry: Arc<Registry>,
    retention: Retention,
}

impl Shared {
    pub fn new(store: Arc<dyn Store>, registry: Arc<Registry>, retention: Retention) -> Self {
        Self {
            store,
            registry,
            retention,
        }
    }
}

#[derive(IntoResponses)]
pub enum Stream {
    #[response(
        status = 200,
        content_type = "application/x-ndjson",
        description = "Stored Atoms in the window, verbatim, one per line",
        headers(
            ("locus-retained-from" = u64, description = "With a declared retention, the instant (ns since epoch) before which history is not retained")
        )
    )]
    Stored,
    #[response(
        status = 400,
        description = "The window, selector or producer is invalid"
    )]
    Invalid(#[to_schema] Fault),
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct Params {
    from: Option<u64>,
    to: Option<u64>,
    role: Option<String>,
    key: Option<String>,
    producer: Option<String>,
}

impl Params {
    fn filter(self) -> Result<Filter, String> {
        let selector = match (self.role, self.key) {
            (Some(role), Some(key)) => Some((
                Role::new(role).map_err(|error| error.to_string())?,
                Key::new(key).map_err(|error| error.to_string())?,
            )),
            (None, None) => None,
            _ => return Err("role and key select together".to_string()),
        };
        if let (Some(from), Some(to)) = (self.from, self.to)
            && from > to
        {
            return Err("window starts after it ends".to_string());
        }
        Ok(Filter {
            window: Window {
                from: self.from,
                to: self.to,
            },
            selector,
            producer: self.producer.as_deref().map(Producer::new).transpose()?,
        })
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "locus-api",
        description = "Faithful Atom recording and retrieval"
    ),
    paths(health, atoms, spools),
    components(schemas(Health, Fault, Detail, Enrollment))
)]
pub struct Document;

pub fn document() -> String {
    Document::openapi()
        .to_pretty_json()
        .expect("OpenAPI document serializes")
}

pub async fn serve(
    listener: TcpListener,
    shared: Shared,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    axum::serve(listener, router(shared))
        .with_graceful_shutdown(shutdown)
        .await
}

fn router(shared: Shared) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/atoms", get(atoms))
        .route("/api/v1/spools", post(spools))
        .fallback(missing)
        .with_state(shared)
}

#[utoipa::path(get, path = "/health", responses((status = 200, body = Health)))]
async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[utoipa::path(get, path = "/api/v1/atoms", params(Params), responses(Stream))]
async fn atoms(State(shared): State<Shared>, Query(params): Query<Params>) -> Response {
    let floor = shared.retention.floor(retention::now());
    let store = shared.store;
    let filter = match params.filter() {
        Ok(filter) => filter,
        Err(message) => return fault(StatusCode::BAD_REQUEST, "request.invalid", message),
    };
    let (sender, receiver) = mpsc::channel::<io::Result<Bytes>>(16);
    tokio::task::spawn_blocking(move || {
        let mut buffer = Vec::with_capacity(CHUNK);
        let flush = |buffer: &mut Vec<u8>| {
            sender
                .blocking_send(Ok(Bytes::from(std::mem::take(buffer))))
                .map_err(|_| "client left".to_string())
        };
        let result = store.read(&filter, &mut |line| {
            buffer.extend_from_slice(line);
            if buffer.len() >= CHUNK {
                flush(&mut buffer)?;
            }
            Ok(())
        });
        let result = result.and_then(|()| flush(&mut buffer));
        if let Err(message) = result {
            let _ = sender.blocking_send(Err(io::Error::other(message)));
        }
    });
    let mut response = (
        [(header::CONTENT_TYPE, "application/x-ndjson")],
        Body::from_stream(ReceiverStream::new(receiver)),
    )
        .into_response();
    if let Some(floor) = floor {
        response
            .headers_mut()
            .insert(RETAINED, HeaderValue::from(floor));
    }
    response
}

#[utoipa::path(
    post,
    path = "/api/v1/spools",
    request_body = Enrollment,
    responses(
        (status = 204, description = "The spool is registered and will be drained"),
        (status = 400, body = Fault)
    )
)]
async fn spools(State(shared): State<Shared>, Json(enrollment): Json<Enrollment>) -> Response {
    match shared
        .registry
        .enroll(&enrollment.producer, Path::new(&enrollment.path))
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(message) => fault(StatusCode::BAD_REQUEST, "spool.invalid", message),
    }
}

async fn missing() -> Response {
    fault(
        StatusCode::NOT_FOUND,
        "route.missing",
        "no such route".to_string(),
    )
}

fn fault(status: StatusCode, code: &'static str, message: String) -> Response {
    (
        status,
        Json(Fault {
            fault: Detail { code, message },
        }),
    )
        .into_response()
}
