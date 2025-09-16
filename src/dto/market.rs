use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::models::{Kline, Symbol};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KlinesParams {
    pub symbol: String,
    pub interval: String,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KlineResponse {
    pub open_time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub close_time: i64,
}

impl From<Kline> for KlineResponse {
    fn from(value: Kline) -> Self {
        Self {
            open_time: value.open_time.0,
            open: value.open.0,
            high: value.high.0,
            low: value.low.0,
            close: value.close.0,
            volume: value.volume.0,
            close_time: value.close_time.0,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInfo {
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
}

impl From<Symbol> for SymbolInfo {
    fn from(value: Symbol) -> Self {
        Self {
            symbol: value.symbol,
            base_asset: value.base,
            quote_asset: value.quote,
            status: if value.active {
                "TRADING".into()
            } else {
                "BREAK".into()
            },
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExchangeInfoResponse {
    pub symbols: Vec<SymbolInfo>,
}
