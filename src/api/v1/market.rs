use axum::{Extension, Json, Router, routing::get};
use std::str::FromStr;
use tracing::instrument;

use crate::{
    api::errors::ApiResult,
    app::bootstrap::AppState,
    domain::value_objects::{Interval, TimestampMs},
    dto::market::{ExchangeInfoResponse, KlineResponse, KlinesParams, SymbolInfo},
};

pub fn router() -> Router {
    Router::new()
        .route("/api/v1/exchangeInfo", get(exchange_info))
        .route("/api/v3/klines", get(klines))
}

#[utoipa::path(
    get,
    path = "/api/v1/exchangeInfo",
    responses((status = 200, body = ExchangeInfoResponse))
)]
#[instrument(skip(state))]
pub async fn exchange_info(
    Extension(state): Extension<AppState>,
) -> ApiResult<Json<ExchangeInfoResponse>> {
    let symbols = state.market_service.exchange_info().await?;
    let response = ExchangeInfoResponse {
        symbols: symbols.into_iter().map(SymbolInfo::from).collect(),
    };
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/v3/klines",
    params(
        ("symbol" = String, Query, description = "Trading pair"),
        ("interval" = String, Query, description = "Kline interval"),
        ("startTime" = i64, Query, description = "Start time in ms"),
        ("endTime" = i64, Query, description = "End time in ms"),
        ("limit" = usize, Query, description = "Max klines")
    ),
    responses((status = 200, body = Vec<KlineResponse>))
)]
#[instrument(skip(state, params))]
pub async fn klines(
    Extension(state): Extension<AppState>,
    params: axum::extract::Query<KlinesParams>,
) -> ApiResult<Json<Vec<KlineResponse>>> {
    let params = params.0;
    let interval = Interval::from_str(&params.interval)?;
    let start = params.start_time.map(TimestampMs::from);
    let end = params.end_time.map(TimestampMs::from);
    let klines = state
        .market_service
        .klines(&params.symbol, interval, start, end, params.limit)
        .await?;
    Ok(Json(klines.into_iter().map(KlineResponse::from).collect()))
}
