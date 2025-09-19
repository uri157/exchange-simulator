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
    pub status: String,   // "registered" | "ingesting" | "ready" | "failed"
    pub created_at: i64,  // epoch ms
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
