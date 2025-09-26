use axum::{http::StatusCode, response::IntoResponse};

use crate::{dto::v3::error::BinanceErrorResponse, error::AppError};

pub fn binance_error(err: AppError) -> axum::response::Response {
    match err {
        AppError::Validation(msg) => IntoResponse::into_response(
            BinanceErrorResponse::new(-1102, msg).into_response(StatusCode::BAD_REQUEST),
        ),
        AppError::NotFound(msg) => IntoResponse::into_response(
            BinanceErrorResponse::new(-2013, msg).into_response(StatusCode::NOT_FOUND),
        ),
        AppError::Conflict(msg) => IntoResponse::into_response(
            BinanceErrorResponse::new(-2010, msg).into_response(StatusCode::CONFLICT),
        ),
        AppError::Database(msg) | AppError::External(msg) | AppError::Internal(msg) => {
            IntoResponse::into_response(
                BinanceErrorResponse::new(-1000, msg)
                    .into_response(StatusCode::INTERNAL_SERVER_ERROR),
            )
        }
    }
}
