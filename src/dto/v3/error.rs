use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
#[allow(unused_imports)]
use serde_json::json;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[schema(
    example = json!({
        "code": -1102,
        "msg": "Mandatory parameter was not sent, was empty/null, or malformed."
    })
)]
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
