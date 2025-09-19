use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub mod dataset_status;

use crate::domain::value_objects::{Interval, Price, Quantity, Speed, TimestampMs};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Symbol {
    pub symbol: String,
    pub base: String,
    pub quote: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Kline {
    pub symbol: String,
    pub interval: Interval,
    pub open_time: TimestampMs,
    pub open: Price,
    pub high: Price,
    pub low: Price,
    pub close: Price,
    pub volume: Quantity,
    pub close_time: TimestampMs,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Trade {
    pub symbol: String,
    pub price: Price,
    pub quantity: Quantity,
    pub timestamp: TimestampMs,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderType {
    Market,
    Limit,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderStatus {
    New,
    Filled,
    PartiallyFilled,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Order {
    pub order_id: Uuid,
    pub session_id: Uuid,
    pub client_order_id: Option<String>,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub price: Option<Price>,
    pub quantity: Quantity,
    pub filled_quantity: Quantity,
    pub status: OrderStatus,
    pub created_at: TimestampMs,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Fill {
    pub order_id: Uuid,
    pub symbol: String,
    pub price: Price,
    pub quantity: Quantity,
    pub fee: Price,
    pub trade_time: TimestampMs,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Balance {
    pub asset: String,
    pub free: Quantity,
    pub locked: Quantity,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountSnapshot {
    pub session_id: Uuid,
    pub balances: Vec<Balance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub enum SessionStatus {
    Created,
    Running,
    Paused,
    Ended,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionConfig {
    pub session_id: Uuid,
    pub symbols: Vec<String>,
    pub interval: Interval,
    pub start_time: TimestampMs,
    pub end_time: TimestampMs,
    pub speed: Speed,
    pub status: SessionStatus,
    pub seed: u64,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
}

// === Nuevo esquema de datasets (fuente: Binance) ===
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DatasetMetadata {
    pub id: Uuid,
    pub symbol: String,
    pub interval: String, // e.g. "1m","1h","1d"
    pub start_time: i64,  // epoch ms
    pub end_time: i64,    // epoch ms
    pub status: String,   // "registered" | "ingesting" | "ready" | "failed"
    pub created_at: i64,  // epoch ms
}

// (Opcional) Compat con ingestiones locales antiguas.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DatasetFormat {
    Csv,
    Parquet,
}

impl std::fmt::Display for DatasetFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatasetFormat::Csv => write!(f, "csv"),
            DatasetFormat::Parquet => write!(f, "parquet"),
        }
    }
}

impl std::str::FromStr for DatasetFormat {
    type Err = crate::error::AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "csv" => Ok(DatasetFormat::Csv),
            "parquet" => Ok(DatasetFormat::Parquet),
            other => Err(crate::error::AppError::Validation(format!(
                "unsupported dataset format: {other}"
            ))),
        }
    }
}
