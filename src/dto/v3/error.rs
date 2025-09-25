use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct BinanceErrorResponse {
    pub code: i32,
    pub msg: String,
}

impl BinanceErrorResponse {
    pub fn new(code: i32, msg: impl Into<String>) -> Self {
        Self {
            code,
            msg: msg.into(),
        }
    }

    pub fn into_response(self, status: StatusCode) -> impl IntoResponse {
        (status, Json(self))
    }
}
