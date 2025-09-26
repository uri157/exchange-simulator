use axum::http::HeaderMap;
use uuid::Uuid;

use crate::{
    dto::v3::orders::{BinanceOrderSide, BinanceOrderType, BinanceTimeInForce, NewOrderRespType},
    error::AppError,
};

pub fn extract_session_id(
    headers: &HeaderMap,
    from_params: Option<&str>,
) -> Result<Uuid, AppError> {
    if let Some(value) = from_params {
        return Uuid::parse_str(value)
            .map_err(|_| AppError::Validation("invalid sessionId".into()));
    }
    if let Some(header_value) = headers.get("X-Session-Id") {
        let value = header_value
            .to_str()
            .map_err(|_| AppError::Validation("invalid X-Session-Id header".into()))?;
        return Uuid::parse_str(value)
            .map_err(|_| AppError::Validation("invalid sessionId".into()));
    }
    Err(AppError::Validation("sessionId is required".into()))
}

pub fn parse_side(value: &str) -> Result<BinanceOrderSide, AppError> {
    serde_json::from_str(&format!("\"{}\"", value.to_ascii_uppercase()))
        .map_err(|_| AppError::Validation("invalid side".into()))
}

pub fn parse_order_type(value: &str) -> Result<BinanceOrderType, AppError> {
    serde_json::from_str(&format!("\"{}\"", value.to_ascii_uppercase()))
        .map_err(|_| AppError::Validation("invalid type".into()))
}

pub fn parse_time_in_force(value: &str) -> Result<BinanceTimeInForce, AppError> {
    serde_json::from_str(&format!("\"{}\"", value.to_ascii_uppercase()))
        .map_err(|_| AppError::Validation("unsupported timeInForce".into()))
}

pub fn parse_resp_type(value: &str) -> Result<NewOrderRespType, AppError> {
    serde_json::from_str(&format!("\"{}\"", value.to_ascii_uppercase()))
        .map_err(|_| AppError::Validation("invalid newOrderRespType".into()))
}

pub fn parse_decimal(value: &str) -> Result<f64, AppError> {
    value
        .parse::<f64>()
        .map_err(|_| AppError::Validation(format!("invalid decimal: {value}")))
}

pub fn required<'a>(value: &'a Option<String>, name: &str) -> Result<&'a str, AppError> {
    value
        .as_deref()
        .ok_or_else(|| AppError::Validation(format!("{name} is required")))
}
