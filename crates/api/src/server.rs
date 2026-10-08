use crate::store::{Filter, Producer, Store, Window};
use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use locus::{Key, Role};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::io;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use utoipa::{IntoParams, OpenApi, ToSchema};

const CHUNK: usize = 64 * 1024;

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
    paths(health, atoms),
    components(schemas(Health, Fault, Detail))
)]
pub struct Document;

pub fn document() -> String {
    Document::openapi()
        .to_pretty_json()
        .expect("OpenAPI document serializes")
}

pub async fn serve(
    listener: TcpListener,
    store: Arc<dyn Store>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    axum::serve(listener, router(store))
        .with_graceful_shutdown(shutdown)
        .await
}

fn router(store: Arc<dyn Store>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/atoms", get(atoms))
        .fallback(missing)
        .with_state(store)
}

#[utoipa::path(get, path = "/health", responses((status = 200, body = Health)))]
async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[utoipa::path(
    get,
    path = "/api/v1/atoms",
    params(Params),
    responses(
        (status = 200, content_type = "application/x-ndjson", description = "Stored Atoms in the window, verbatim, one per line"),
        (status = 400, body = Fault)
    )
)]
async fn atoms(State(store): State<Arc<dyn Store>>, Query(params): Query<Params>) -> Response {
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
    (
        [(header::CONTENT_TYPE, "application/x-ndjson")],
        Body::from_stream(ReceiverStream::new(receiver)),
    )
        .into_response()
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
