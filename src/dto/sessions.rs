use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::domain::models::{SessionConfig, SessionStatus};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub symbols: Vec<String>,
    pub interval: String,
    pub start_time: i64,
    pub end_time: i64,
    pub speed: Option<f64>,
    pub seed: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponse {
    pub id: Uuid,
    pub symbols: Vec<String>,
    pub interval: String,
    pub start_time: i64,
    pub end_time: i64,
    pub speed: f64,
    pub status: SessionStatus,
    pub seed: u64,
    pub created_at: i64,
}

impl From<SessionConfig> for SessionResponse {
    fn from(value: SessionConfig) -> Self {
        Self {
            id: value.session_id,
            symbols: value.symbols,
            interval: value.interval.as_str().to_string(),
            start_time: value.start_time.0,
            end_time: value.end_time.0,
            speed: value.speed.0,
            status: value.status,
            seed: value.seed,
            created_at: value.created_at.0,
        }
    }
}
