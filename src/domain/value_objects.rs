use std::{fmt, ops::Deref, str::FromStr};

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct TimestampMs(pub i64);

impl TimestampMs {
    pub fn as_datetime(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.0)
            .single()
            .expect("invalid timestamp")
    }
}

impl Deref for TimestampMs {
    type Target = i64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<i64> for TimestampMs {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct Price(pub f64);

impl Deref for Price {
    type Target = f64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<f64> for Price {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct Quantity(pub f64);

impl Default for Quantity {
    fn default() -> Self {
        Self(0.0)
    }
}

impl Deref for Quantity {
    type Target = f64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<f64> for Quantity {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct Interval(pub String);

impl Interval {
    pub fn new<S: Into<String>>(value: S) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for Interval {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for Interval {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(AppError::Validation("interval cannot be empty".into()));
        }
        Ok(Self(s.to_string()))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct Speed(pub f64);

impl Speed {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.0 <= 0.0 {
            return Err(AppError::Validation("speed must be positive".into()));
        }
        Ok(())
    }
}

impl From<f64> for Speed {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct DatasetPath(pub String);

impl Deref for DatasetPath {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<String> for DatasetPath {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for DatasetPath {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl DatasetPath {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
