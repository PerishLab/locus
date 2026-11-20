use super::identity::Authority;
use super::{Params, RETAINED, health};
use crate::column::Columns;
use crate::retention::{self, Retention};
use crate::store::{Producer, Record};
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use locus::Atom;
use serde::Deserialize;
use std::{io, sync::Arc};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Clone)]
struct Gate {
    authority: Arc<Authority>,
    columns: Arc<Columns>,
    retention: Retention,
}

pub fn edge(columns: Arc<Columns>, authority: Authority, retention: Retention) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/atoms", get(scan).post(append))
        .with_state(Gate {
            authority: Arc::new(authority),
            columns,
            retention,
        })
}

impl Gate {
    async fn actor(&self, headers: &HeaderMap) -> Result<i64, StatusCode> {
        let mut keys = headers.get_all(header::AUTHORIZATION).iter();
        let key = keys
            .next()
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .filter(|v| !v.is_empty() && !v.bytes().any(|b| b.is_ascii_whitespace()))
            .ok_or(StatusCode::UNAUTHORIZED)?
            .to_owned();
        if keys.next().is_some() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let authority = self.authority.clone();
        let owner = tokio::task::spawn_blocking(move || authority.exchange(&key))
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)??;
        self.columns.operator(&owner).await
    }
}

#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct Source {
    producer: String,
}

#[utoipa::path(post,path="/api/v1/atoms", params(Source,("Idempotency-Key" = String,Header,description="Stable label for this batch")),request_body(content=String,content_type="application/x-ndjson",description="Complete Atom lines for the named producer"),responses((status=204,description="Accepted by the column store on the machine listener"),(status=400,description="Invalid Atom batch"),(status=401,description="Machine credential rejected"),(status=403,description="Source grant refused"),(status=503,description="Authorization or storage unavailable")))]
async fn append(
    State(gate): State<Gate>,
    Query(source): Query<Source>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let who = gate.actor(&headers).await?;
    let producer = Producer::new(&source.producer).map_err(|_| StatusCode::BAD_REQUEST)?;
    let token = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.trim().is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let data = body.strip_suffix(b"\n").ok_or(StatusCode::BAD_REQUEST)?;
    let mut records = Vec::new();
    for line in data.split(|c| *c == b'\n') {
        let atom: Atom = serde_json::from_slice(line).map_err(|_| StatusCode::BAD_REQUEST)?;
        if atom.producer() != Some(producer.text()) || atom.id().is_none_or(str::is_empty) {
            return Err(StatusCode::BAD_REQUEST);
        }
        records.push(Record::encode(line, &atom));
    }
    gate.columns
        .submit(who, &producer, &records, token)
        .await
        .map_err(status)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn scan(
    State(gate): State<Gate>,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let who = gate.actor(&headers).await?;
    let filter = params.filter().map_err(|_| StatusCode::BAD_REQUEST)?;
    if filter.producer.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (sender, mut receiver) = mpsc::channel(8);
    let worker = tokio::task::spawn_blocking(move || {
        let result = pollster::block_on(gate.columns.scan(who, &filter, &mut |raw| {
            sender
                .blocking_send(Ok(Bytes::copy_from_slice(raw)))
                .map_err(|_| "client left".to_owned())
        }));
        if let Err(error) = result {
            let _ = sender.blocking_send(Err(status(error)));
        }
    });
    let first = receiver.recv().await.transpose()?;
    if first.is_none() {
        worker.await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    }

    use tokio_stream::StreamExt;
    let body = tokio_stream::iter(first.into_iter().map(Ok::<_, io::Error>)).chain(
        ReceiverStream::new(receiver)
            .map(|it| it.map_err(|status| io::Error::other(status.to_string()))),
    );
    let mut response = (
        [(header::CONTENT_TYPE, "application/x-ndjson")],
        Body::from_stream(body),
    )
        .into_response();
    if let Some(floor) = gate.retention.floor(retention::now()) {
        response
            .headers_mut()
            .insert(RETAINED, HeaderValue::from(floor));
    }
    Ok(response)
}

fn status(error: keel_column::Error) -> StatusCode {
    if error.refused() {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
