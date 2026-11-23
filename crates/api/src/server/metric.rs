use super::ingress::{Gate, status};
use crate::column::Columns;
use crate::store::{Filter, Producer, Window};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use locus_derivation::metric::{Metric, Query as Selection, Report};
use std::sync::Arc;

#[utoipa::path(get, path = "/api/v1/metrics", params(Selection), responses(
    (status = 200, body = Report, description = "Source-authorized gauge means or counter increases with immutable evidence"),
    (status = 400, description = "Invalid descriptor or window"),
    (status = 401, description = "Credential rejected"),
    (status = 403, description = "Producer grant refused"),
    (status = 422, description = "Sample contract or bounded query budget exceeded"),
    (status = 503, description = "Authorization or storage unavailable")
))]
pub(super) async fn read(
    State(gate): State<Gate>,
    headers: HeaderMap,
    Query(query): Query<Selection>,
) -> Result<Json<Report>, StatusCode> {
    let who = gate.reader(&headers).await?;
    evaluate(gate.columns, who, query).await.map(Json)
}

pub(super) async fn evaluate(
    columns: Arc<Columns>,
    who: i64,
    query: Selection,
) -> Result<Report, StatusCode> {
    let producer = Producer::new(&query.producer).map_err(|_| StatusCode::BAD_REQUEST)?;
    let filter = Filter {
        window: Window {
            from: Some(query.from),
            to: Some(query.to),
        },
        producer: Some(producer),
        selector: None,
    };
    let mut metric = Metric::new(query).map_err(|_| StatusCode::BAD_REQUEST)?;
    tokio::task::spawn_blocking(move || {
        let mut invalid = false;
        let result = pollster::block_on(columns.scan(who, &filter, &mut |raw| {
            metric.observe(raw).inspect_err(|_| invalid = true)
        }));
        if invalid {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        result.map_err(status)?;
        Ok(metric.finish())
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
}
