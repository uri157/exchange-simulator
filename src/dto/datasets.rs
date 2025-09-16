use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::domain::models::{DatasetFormat, DatasetMetadata};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDatasetRequest {
    pub name: String,
    pub path: String,
    pub format: DatasetFormat,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DatasetResponse {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub format: DatasetFormat,
    pub created_at: i64,
}

impl From<DatasetMetadata> for DatasetResponse {
    fn from(value: DatasetMetadata) -> Self {
        Self {
            id: value.id,
            name: value.name,
            path: value.base_path.as_str().to_string(),
            format: value.format,
            created_at: value.created_at.0,
        }
    }
}
