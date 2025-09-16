use axum::{Json, response::IntoResponse};
use serde::Serialize;

use crate::error::AppError;

pub type ApiResult<T> = Result<T, AppError>;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

pub async fn ping() -> impl IntoResponse {
    Json(HealthResponse { status: "ok" })
}
