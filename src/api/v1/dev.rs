use std::str::FromStr;

use axum::{routing::post, Extension, Json, Router};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    api::errors::ApiResult,
    app::bootstrap::AppState,
    domain::value_objects::{Interval, TimestampMs},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedAggTradesRequest {
    pub symbol: String,
    pub interval: String,
    pub from: i64,
    pub to: i64,
    pub trades_per_kline: usize,
    pub seed: u64,
}

#[derive(Serialize)]
pub struct SeedAggTradesResponse {
    pub inserted: u64,
}

fn dev_endpoints_enabled() -> bool {
    cfg!(debug_assertions) || matches!(std::env::var("ENABLE_DEV_ENDPOINTS"), Ok(val) if val == "1")
}

pub fn router() -> Router {
    if dev_endpoints_enabled() {
        Router::new().route(
            "/api/v1/dev/seed-aggtrades",
            post(seed_aggtrades_from_klines),
        )
    } else {
        Router::new()
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/dev/seed-aggtrades",
    request_body = SeedAggTradesRequest,
    responses(
        (status = 200, description = "Synthetic aggtrades seeded", body = SeedAggTradesResponse)
    )
)]
#[instrument(skip(state, payload))]
pub async fn seed_aggtrades_from_klines(
    Extension(state): Extension<AppState>,
    Json(payload): Json<SeedAggTradesRequest>,
) -> ApiResult<Json<SeedAggTradesResponse>> {
    let interval = Interval::from_str(&payload.interval)?;

    let inserted = state
        .ingest_service
        .seed_aggtrades_from_klines(
            &payload.symbol,
            interval,
            TimestampMs(payload.from),
            TimestampMs(payload.to),
            payload.trades_per_kline,
            payload.seed,
        )
        .await?;

    Ok(Json(SeedAggTradesResponse { inserted }))
}
