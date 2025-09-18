use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

fn client() -> Client {
    Client::builder()
        .user_agent("exchange-simulator/binance-infra")
        .build()
        .expect("reqwest client")
}

#[derive(Deserialize)]
struct ExchangeInfo {
    symbols: Vec<BSymbol>,
}

#[derive(Deserialize)]
struct BSymbol {
    symbol: String,
    #[serde(rename = "baseAsset")]
    base_asset: String,
    #[serde(rename = "quoteAsset")]
    quote_asset: String,
    status: String,
    #[serde(rename = "isSpotTradingAllowed", default)]
    is_spot_trading_allowed: Option<bool>,
}

/// Tipo simple para exponer símbolos desde la capa infra (sin depender del módulo api).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub symbol: String,
    pub base: String,
    pub quote: String,
    pub active: bool,
}

/// Rango disponible de velas para un par/intervalo.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AvailableRange {
    pub symbol: String,
    pub interval: String,
    pub first_open_time: i64,
    pub last_close_time: i64,
}

/// Devuelve la lista de símbolos spot activos desde Binance.
pub async fn fetch_symbols() -> Result<Vec<SymbolInfo>, AppError> {
    let resp = client()
        .get("https://api.binance.com/api/v3/exchangeInfo")
        .send()
        .await
        .map_err(|e| AppError::External(format!("exchangeInfo request failed: {e}")))?;

    let info: ExchangeInfo = resp
        .json()
        .await
        .map_err(|e| AppError::External(format!("exchangeInfo parse failed: {e}")))?;

    let out = info
        .symbols
        .into_iter()
        .filter(|s| s.status == "TRADING" && s.is_spot_trading_allowed.unwrap_or(true))
        .map(|s| SymbolInfo {
            symbol: s.symbol,
            base: s.base_asset,
            quote: s.quote_asset,
            active: true,
        })
        .collect();

    Ok(out)
}

/// Obtiene el primer open_time y el último close_time disponible para `symbol`/`interval`.
pub async fn fetch_available_range(symbol: &str, interval: &str) -> Result<AvailableRange, AppError> {
    let base = "https://api.binance.com/api/v3/klines";
    let now = Utc::now().timestamp_millis();

    // Primera vela (startTime=0, limit=1)
    let url_first = format!("{base}?symbol={symbol}&interval={interval}&startTime=0&limit=1");
    let first: Vec<Vec<serde_json::Value>> = client()
        .get(&url_first)
        .send()
        .await
        .map_err(|e| AppError::External(format!("klines first req failed: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::External(format!("klines first parse failed: {e}")))?;

    // Última vela (endTime=now, limit=1)
    let url_last = format!("{base}?symbol={symbol}&interval={interval}&endTime={now}&limit=1");
    let last: Vec<Vec<serde_json::Value>> = client()
        .get(&url_last)
        .send()
        .await
        .map_err(|e| AppError::External(format!("klines last req failed: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::External(format!("klines last parse failed: {e}")))?;

    let first_open_time = first
        .get(0)
        .and_then(|r| r.get(0))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let last_close_time = last
        .get(0)
        .and_then(|r| r.get(6))
        .and_then(|v| v.as_i64())
        .unwrap_or(now);

    Ok(AvailableRange {
        symbol: symbol.to_string(),
        interval: interval.to_string(),
        first_open_time,
        last_close_time,
    })
}
