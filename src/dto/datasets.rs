use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::domain::models::DatasetMetadata;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateDatasetRequest {
    pub symbol: String,
    pub interval: String, // e.g. "1m","1h","1d"
    pub start_time: i64,  // epoch ms
    pub end_time: i64,    // epoch ms
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DatasetResponse {
    pub id: Uuid,
    pub symbol: String,
    pub interval: String,
    pub start_time: i64,
    pub end_time: i64,
    pub status: String,  // "registered" | "ingesting" | "ready" | "failed"
    pub created_at: i64, // epoch ms
}

impl From<DatasetMetadata> for DatasetResponse {
    fn from(value: DatasetMetadata) -> Self {
        Self {
            id: value.id,
            symbol: value.symbol,
            interval: value.interval,
            start_time: value.start_time,
            end_time: value.end_time,
            status: value.status,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SymbolOnly {
    pub symbol: String,
}

impl From<String> for SymbolOnly {
    fn from(symbol: String) -> Self {
        Self { symbol }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IntervalOnly {
    pub interval: String,
}

impl From<String> for IntervalOnly {
    fn from(interval: String) -> Self {
        Self { interval }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RangeResponse {
    pub first_open_time: i64,
    pub last_close_time: i64,
}

impl From<(i64, i64)> for RangeResponse {
    fn from(range: (i64, i64)) -> Self {
        Self {
            first_open_time: range.0,
            last_close_time: range.1,
        }
    }
}
