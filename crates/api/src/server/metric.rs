use super::ingress::{Gate, status};
use crate::store::{Filter, Producer, Window};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use locus_derivation::metric::{Metric, Query as Selection, Report};

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
    let who = gate.actor(&headers).await?;
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
        let result = pollster::block_on(gate.columns.scan(who, &filter, &mut |raw| {
            metric.observe(raw).inspect_err(|_| invalid = true)
        }));
        if invalid {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        result.map_err(status)?;
        Ok(Json(metric.finish()))
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
}
