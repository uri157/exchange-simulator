use std::collections::HashMap;

use axum::{
    extract::RawQuery,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Extension, Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json;
use tracing::instrument;
use uuid::Uuid;

use crate::{
    app::bootstrap::AppState,
    dto::{
        account::{AccountQuery, AccountResponse},
        v3::{
            account::{BinanceAccountResponse, BinanceBalance},
            error::BinanceErrorResponse,
        },
    },
    error::AppError,
};

pub fn router() -> Router {
    Router::new().route("/api/v3/account", get(get_account))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceAccountParams {
    timestamp: Option<String>,
    recv_window: Option<String>,
    session_id: Option<String>,
    signature: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v3/account",
    params(crate::dto::account::AccountQuery),
    responses((status = 200, body = crate::dto::account::AccountResponse))
)]
#[instrument(skip(state, raw_query))]
pub async fn get_account(
    Extension(state): Extension<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Result<axum::response::Response, AppError> {
    let params_map = parse_query_map(raw_query.as_deref())?;
    if is_binance_request(&params_map) {
        match handle_binance_account(&state, &headers, params_map).await {
            Ok(resp) => Ok(Json(resp).into_response()),
            Err(err) => Ok(binance_error(err)),
        }
    } else {
        let params: AccountQuery =
            serde_urlencoded::from_str(raw_query.as_deref().unwrap_or_default())
                .map_err(|err| AppError::Validation(format!("invalid query params: {err}")))?;
        state
            .account_service
            .ensure_session_account(params.session_id)
            .await?;
        let account = state.account_service.get_account(params.session_id).await?;
        Ok(Json(AccountResponse::from(account)).into_response())
    }
}

async fn handle_binance_account(
    state: &AppState,
    headers: &HeaderMap,
    params_map: HashMap<String, String>,
) -> Result<BinanceAccountResponse, AppError> {
    let params: BinanceAccountParams = map_to_struct(params_map)?;
    let session_id = extract_session_id(headers, params.session_id.as_deref())?;
    state
        .account_service
        .ensure_session_account(session_id)
        .await?;
    let account = state.account_service.get_account(session_id).await?;
    let balances = account
        .balances
        .into_iter()
        .map(|balance| BinanceBalance {
            asset: balance.asset,
            free: format_decimal(balance.free.0),
            locked: format_decimal(balance.locked.0),
        })
        .collect();

    Ok(BinanceAccountResponse {
        maker_commission: 0,
        taker_commission: 0,
        buyer_commission: 0,
        seller_commission: 0,
        can_trade: true,
        can_withdraw: false,
        can_deposit: false,
        brokered: false,
        update_time: Utc::now().timestamp_millis(),
        account_type: "SPOT".to_string(),
        balances,
        permissions: vec!["SPOT".to_string()],
    })
}

fn parse_query_map(raw_query: Option<&str>) -> Result<HashMap<String, String>, AppError> {
    if let Some(query) = raw_query {
        if query.is_empty() {
            return Ok(HashMap::new());
        }
        let pairs: Vec<(String, String)> = serde_urlencoded::from_str(query)
            .map_err(|err| AppError::Validation(format!("invalid query params: {err}")))?;
        Ok(pairs.into_iter().collect())
    } else {
        Ok(HashMap::new())
    }
}

fn is_binance_request(params: &HashMap<String, String>) -> bool {
    params.contains_key("timestamp") || params.contains_key("recvWindow")
}

fn map_to_struct<T>(map: HashMap<String, String>) -> Result<T, AppError>
where
    T: for<'de> Deserialize<'de>,
{
    let value = serde_json::to_value(map)
        .map_err(|err| AppError::Validation(format!("failed to convert params: {err}")))?;
    serde_json::from_value(value)
        .map_err(|err| AppError::Validation(format!("invalid parameters: {err}")))
}

fn extract_session_id(headers: &HeaderMap, from_params: Option<&str>) -> Result<Uuid, AppError> {
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

fn format_decimal(value: f64) -> String {
    format!("{:.8}", value)
}

pub fn symbol_components(symbol: &str, default_quote: &str) -> (String, String) {
    const COMMON_QUOTES: [&str; 6] = ["USDT", "USD", "BUSD", "USDC", "BTC", "ETH"];
    for quote in COMMON_QUOTES.iter().chain(std::iter::once(&default_quote)) {
        if let Some(base) = symbol.strip_suffix(*quote) {
            if !base.is_empty() {
                return (base.to_string(), (*quote).to_string());
            }
        }
    }
    let split = symbol.len() / 2;
    (symbol[..split].to_string(), symbol[split..].to_string())
}

fn binance_error(err: AppError) -> axum::response::Response {
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
