use std::{fmt, str::FromStr};

use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize, Serializer};
use utoipa::ToSchema;

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToSchema)]
pub enum DatasetStatus {
    Registered,
    Ingesting,
    Ready,
    Failed,
    Canceled,
}

impl fmt::Display for DatasetStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_api_str())
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
            "canceled" | "cancelled" => Ok(DatasetStatus::Canceled),
            other => Err(AppError::Validation(format!(
                "invalid dataset status: {other}"
            ))),
        }
    }
}

impl DatasetStatus {
    pub const fn as_storage_str(&self) -> &'static str {
        match self {
            DatasetStatus::Registered => "registered",
            DatasetStatus::Ingesting => "ingesting",
            DatasetStatus::Ready => "ready",
            DatasetStatus::Failed => "failed",
            DatasetStatus::Canceled => "canceled",
        }
    }

    pub const fn as_api_str(&self) -> &'static str {
        match self {
            DatasetStatus::Registered => "Registered",
            DatasetStatus::Ingesting => "Ingesting",
            DatasetStatus::Ready => "Ready",
            DatasetStatus::Failed => "Failed",
            DatasetStatus::Canceled => "Canceled",
        }
    }

    pub const fn as_str(&self) -> &'static str {
        self.as_storage_str()
    }
}

impl Serialize for DatasetStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_api_str())
    }
}

impl<'de> Deserialize<'de> for DatasetStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        DatasetStatus::from_str(&value).map_err(DeError::custom)
    }
}
