use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::domain::models::{dataset_status::DatasetStatus, DatasetMetadata};

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
    pub status: DatasetStatus,
    pub progress: u8,
    pub last_message: Option<String>,
    pub created_at: i64, // epoch ms
    pub updated_at: i64, // epoch ms
}

impl From<DatasetMetadata> for DatasetResponse {
    fn from(meta: DatasetMetadata) -> Self {
        Self {
            id: meta.id,
            symbol: meta.symbol,
            interval: meta.interval,
            start_time: meta.start_time,
            end_time: meta.end_time,
            status: meta.status,
            progress: meta.progress,
            last_message: meta.last_message,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DatasetDetail {
    pub id: Uuid,
    pub symbol: String,
    pub interval: String,
    pub start_time: i64,
    pub end_time: i64,
    pub status: DatasetStatus,
    pub progress: u8,
    pub last_message: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub logs: Vec<DatasetLogEntry>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DatasetLogEntry {
    pub line: String,
    pub ts: i64,
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
