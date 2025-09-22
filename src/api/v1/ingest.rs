use axum::{routing::post, Extension, Json, Router};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{api::errors::ApiResult, app::bootstrap::AppState, error::AppError};

pub fn router() -> Router {
    Router::new().route("/api/v1/ingest/aggtrades", post(ingest_agg_trades))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IngestAggTradesRequest {
    pub symbol: String,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub clear_before: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IngestAggTradesResponse {
    pub symbol: String,
    pub fetched: usize,
    pub inserted: usize,
    pub skipped: usize,
    pub from_id_start: Option<i64>,
    pub from_id_end: Option<i64>,
    pub t_first: Option<i64>,
    pub t_last: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/v1/ingest/aggtrades",
    request_body = IngestAggTradesRequest,
    responses((status = 200, body = IngestAggTradesResponse)),
    tag = "ingest"
)]
#[instrument(skip(state, payload))]
pub async fn ingest_agg_trades(
    Extension(state): Extension<AppState>,
    Json(payload): Json<IngestAggTradesRequest>,
) -> ApiResult<Json<IngestAggTradesResponse>> {
    if payload.symbol.trim().is_empty() {
        return Err(AppError::Validation("symbol cannot be empty".into()));
    }
    if let (Some(start), Some(end)) = (payload.start_time, payload.end_time) {
        if start > end {
            return Err(AppError::Validation(
                "startTime cannot be greater than endTime".into(),
            ));
        }
    }

    let clear_before = payload.clear_before.unwrap_or(false);
    let result = state
        .ingest_service
        .ingest_agg_trades(
            payload.symbol.clone(),
            payload.start_time,
            payload.end_time,
            clear_before,
        )
        .await?;

    let response = IngestAggTradesResponse {
        symbol: result.symbol,
        fetched: result.fetched,
        inserted: result.inserted,
        skipped: result.skipped,
        from_id_start: result.from_id_start,
        from_id_end: result.from_id_end,
        t_first: result.t_first,
        t_last: result.t_last,
    };

    Ok(Json(response))
}
