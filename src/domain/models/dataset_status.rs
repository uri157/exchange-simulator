use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DatasetStatus {
    Registered,
    Ingesting,
    Ready,
    Failed,
}

impl fmt::Display for DatasetStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DatasetStatus::Registered => "registered",
            DatasetStatus::Ingesting => "ingesting",
            DatasetStatus::Ready => "ready",
            DatasetStatus::Failed => "failed",
        };
        f.write_str(s)
    }
}

impl FromStr for DatasetStatus {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "registered" => Ok(DatasetStatus::Registered),
            "ingesting" => Ok(DatasetStatus::Ingesting),
            "ready" => Ok(DatasetStatus::Ready),
            "failed" => Ok(DatasetStatus::Failed),
            other => Err(AppError::Validation(format!("invalid dataset status: {other}"))),
        }
    }
}

impl DatasetStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            DatasetStatus::Registered => "registered",
            DatasetStatus::Ingesting => "ingesting",
            DatasetStatus::Ready => "ready",
            DatasetStatus::Failed => "failed",
        }
    }
}
