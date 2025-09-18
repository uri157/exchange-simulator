use axum::{
    extract::Query,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::api::errors::ApiResult;

pub fn router() -> Router {
    Router::new()
        .route("/api/v1/binance/symbols", get(symbols))
        .route("/api/v1/binance/intervals", get(intervals))
        .route("/api/v1/binance/range", get(range))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInfo {
    pub symbol: String,
    pub base: String,
    pub quote: String,
    pub active: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableRange {
    pub symbol: String,
    pub interval: String,
    pub first_open_time: i64, // ms epoch
    pub last_close_time: i64, // ms epoch
}

#[derive(Deserialize)]
pub struct RangeParams {
    pub symbol: String,
    pub interval: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/binance/symbols",
    responses((status = 200, body = Vec<SymbolInfo>))
)]
#[instrument(skip_all)]
pub async fn symbols() -> ApiResult<Json<Vec<SymbolInfo>>> {
    #[derive(Deserialize)]
    struct ExchangeInfo { symbols: Vec<BSymbol> }
    #[derive(Deserialize)]
    struct BSymbol {
        symbol: String,
        #[serde(rename = "baseAsset")] base_asset: String,
        #[serde(rename = "quoteAsset")] quote_asset: String,
        status: String,
        #[serde(rename = "isSpotTradingAllowed", default)]
        is_spot_trading_allowed: Option<bool>,
    }

    let resp = reqwest::Client::new()
        .get("https://api.binance.com/api/v3/exchangeInfo")
        .send()
        .await
        .map_err(|e| crate::error::AppError::External(format!("exchangeInfo request failed: {e}")))?;
    let info: ExchangeInfo = resp.json().await
        .map_err(|e| crate::error::AppError::External(format!("exchangeInfo parse failed: {e}")))?;

    let out = info.symbols.into_iter()
        .filter(|s| s.status == "TRADING" && s.is_spot_trading_allowed.unwrap_or(true))
        .map(|s| SymbolInfo {
            symbol: s.symbol,
            base: s.base_asset,
            quote: s.quote_asset,
            active: true,
        })
        .collect();

    Ok(Json(out))
}

#[utoipa::path(
    get,
    path = "/api/v1/binance/intervals",
    responses((status = 200, body = [String]))
)]
#[instrument(skip_all)]
pub async fn intervals() -> ApiResult<Json<Vec<&'static str>>> {
    Ok(Json(vec![
        "1m","3m","5m","15m","30m","1h","2h","4h","6h","8h","12h","1d","3d","1w","1M",
    ]))
}

#[utoipa::path(
    get,
    path = "/api/v1/binance/range",
    params(
      ("symbol" = String, Query, description = "Símbolo, p. ej. BTCUSDT"),
      ("interval" = String, Query, description = "Temporalidad, p. ej. 1m/1h/1d")
    ),
    responses((status = 200, body = AvailableRange))
)]
#[instrument(skip_all)]
pub async fn range(Query(q): Query<RangeParams>) -> ApiResult<Json<AvailableRange>> {
    let base = "https://api.binance.com/api/v3/klines";
    let now = chrono::Utc::now().timestamp_millis();

    let client = reqwest::Client::new();

    let url_first = format!(
        "{base}?symbol={}&interval={}&startTime=0&limit=1",
        q.symbol, q.interval
    );
    let first: Vec<Vec<serde_json::Value>> = client
        .get(&url_first)
        .send()
        .await
        .map_err(|e| crate::error::AppError::External(format!("klines first req failed: {e}")))?
        .json()
        .await
        .map_err(|e| crate::error::AppError::External(format!("klines first parse failed: {e}")))?;

    let url_last = format!(
        "{base}?symbol={}&interval={}&endTime={}&limit=1",
        q.symbol, q.interval, now
    );
    let last: Vec<Vec<serde_json::Value>> = client
        .get(&url_last)
        .send()
        .await
        .map_err(|e| crate::error::AppError::External(format!("klines last req failed: {e}")))?
        .json()
        .await
        .map_err(|e| crate::error::AppError::External(format!("klines last parse failed: {e}")))?;

    let first_open = first.get(0).and_then(|r| r.get(0)).and_then(|v| v.as_i64()).unwrap_or(0);
    let last_close = last.get(0).and_then(|r| r.get(6)).and_then(|v| v.as_i64()).unwrap_or(now);

    Ok(Json(AvailableRange {
        symbol: q.symbol,
        interval: q.interval,
        first_open_time: first_open,
        last_close_time: last_close,
    }))
}
