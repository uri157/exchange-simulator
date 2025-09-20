use std::str::FromStr;

use axum::{extract::Query, routing::get, Extension, Json, Router};
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
        // Local DuckDB klines (simulador)
        .route("/api/v1/market/klines", get(local_klines))
        // Compat con estilo Binance
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
    path = "/api/v1/market/klines",
    params(
        ("symbol" = String, Query, description = "Símbolo, ej: ETHBTC"),
        ("interval" = String, Query, description = "Intervalo, ej: 1m"),
        ("startTime" = i64, Query, description = "Inicio en epoch ms (opcional)"),
        ("endTime" = i64, Query, description = "Fin en epoch ms (opcional)"),
        ("limit" = usize, Query, description = "Máx. filas (default 1000)")
    ),
    responses((status = 200, body = Vec<KlineResponse>))
)]
#[instrument(skip(state, params))]
pub async fn local_klines(
    Extension(state): Extension<AppState>,
    Query(params): Query<KlinesParams>,
) -> ApiResult<Json<Vec<KlineResponse>>> {
    let interval = Interval::from_str(&params.interval)?;
    let start = params.start_time.map(TimestampMs::from);
    let end = params.end_time.map(TimestampMs::from);
    let limit = params.limit;

    let klines = state
        .market_service
        .klines(&params.symbol, interval, start, end, limit)
        .await?;

    Ok(Json(klines.into_iter().map(KlineResponse::from).collect()))
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
    Query(params): Query<KlinesParams>,
) -> ApiResult<Json<Vec<KlineResponse>>> {
    let interval = Interval::from_str(&params.interval)?;
    let start = params.start_time.map(TimestampMs::from);
    let end = params.end_time.map(TimestampMs::from);
    let limit = params.limit;

    let klines = state
        .market_service
        .klines(&params.symbol, interval, start, end, limit)
        .await?;

    Ok(Json(klines.into_iter().map(KlineResponse::from).collect()))
}
