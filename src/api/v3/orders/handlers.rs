use axum::{
    body::Bytes,
    extract::RawQuery,
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
#[allow(unused_imports)]
use serde_json::json;
use tracing::instrument;

use crate::{
    app::bootstrap::AppState,
    dto::orders::{
        CancelOrderParams, FillResponse, MyTradesParams, NewOrderResponse, OpenOrdersParams,
        OrderResponse, QueryOrderParams,
    },
    error::AppError,
};

use super::{
    adapters::{
        handle_binance_cancel_order, handle_binance_get_order, handle_binance_my_trades,
        handle_binance_new_order, handle_binance_open_orders, is_binance_request,
        parse_new_order_payload, parse_query_map,
    },
    errors::binance_error,
    types::NewOrderPayload,
};

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/v3/order",
            post(new_order).get(get_order).delete(cancel_order),
        )
        .route("/api/v3/openOrders", get(open_orders))
        .route("/api/v3/myTrades", get(my_trades))
}

#[utoipa::path(
    post,
    path = "/api/v3/order",
    request_body(
        content = crate::api::v3::orders::types::BinanceNewOrderParams,
        content_type = "application/x-www-form-urlencoded",
        description = "Form parameters mirror Binance API. Parameters may also be provided via query string; when duplicated, the query value takes precedence.",
        examples(
            ("limit" = (value = json!({
                "symbol": "ETHBTC",
                "side": "BUY",
                "type": "LIMIT",
                "timeInForce": "GTC",
                "quantity": "1",
                "price": "0.01",
                "timestamp": "1730000000000",
                "recvWindow": "5000",
                "sessionId": "00000000-0000-0000-0000-000000000000"
            }))),
            ("marketQuote" = (value = json!({
                "symbol": "ETHBTC",
                "side": "SELL",
                "type": "MARKET",
                "quoteOrderQty": "100",
                "timestamp": "1730000000000",
                "sessionId": "00000000-0000-0000-0000-000000000000"
            })))
        )
    ),
    params(crate::api::v3::orders::types::BinanceNewOrderParams),
    responses(
        (status = 200, body = crate::dto::v3::orders::BinanceNewOrderResponse),
        (
            status = 400,
            description = "Validation error",
            body = crate::dto::v3::error::BinanceErrorResponse,
            examples((
                "missingParameter" = (value = json!({
                    "code": -1102,
                    "msg": "Mandatory parameter was not sent, was empty/null, or malformed."
                }))
            ))
        ),
        (
            status = 409,
            description = "Order conflict",
            body = crate::dto::v3::error::BinanceErrorResponse
        ),
        (
            status = 500,
            description = "Internal error",
            body = crate::dto::v3::error::BinanceErrorResponse
        )
    ),
    tag = "binance-v3"
)]
#[instrument(skip(state, body))]
pub async fn new_order(
    Extension(state): Extension<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Result<axum::response::Response, AppError> {
    match parse_new_order_payload(&headers, raw_query.as_deref(), &body)? {
        NewOrderPayload::Legacy(payload) => {
            let (order, fills) = state
                .orders_service
                .place_order(
                    payload.session_id,
                    payload.symbol.clone(),
                    payload.side,
                    payload.order_type,
                    crate::domain::value_objects::Quantity(payload.quantity),
                    payload.price.map(crate::domain::value_objects::Price::from),
                    payload.client_order_id.clone(),
                )
                .await?;
            state
                .order_id_mapping
                .ensure_mapping(payload.session_id, order.order_id)
                .await;
            let response = NewOrderResponse {
                order: OrderResponse::from(order),
                fills: fills.into_iter().map(FillResponse::from).collect(),
            };
            Ok(Json(response).into_response())
        }
        NewOrderPayload::Binance(params) => {
            match handle_binance_new_order(&state, &headers, raw_query.as_deref(), params).await {
                Ok(resp) => Ok(Json(resp).into_response()),
                Err(err) => Ok(binance_error(err)),
            }
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v3/order",
    params(crate::api::v3::orders::types::BinanceQueryParams),
    responses(
        (status = 200, body = crate::dto::v3::orders::BinanceOrderDetails),
        (
            status = 400,
            description = "Validation error",
            body = crate::dto::v3::error::BinanceErrorResponse,
            examples((
                "missingParameter" = (value = json!({
                    "code": -1102,
                    "msg": "Mandatory parameter was not sent, was empty/null, or malformed."
                }))
            ))
        ),
        (
            status = 404,
            description = "Order not found",
            body = crate::dto::v3::error::BinanceErrorResponse,
            examples((
                "orderNotFound" = (value = json!({
                    "code": -2013,
                    "msg": "Order does not exist"
                }))
            ))
        ),
        (
            status = 500,
            description = "Internal error",
            body = crate::dto::v3::error::BinanceErrorResponse
        )
    ),
    tag = "binance-v3"
)]
#[instrument(skip(state, raw_query))]
pub async fn get_order(
    Extension(state): Extension<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Result<axum::response::Response, AppError> {
    let params_map = parse_query_map(raw_query.as_deref())?;
    if is_binance_request(&params_map) {
        match handle_binance_get_order(&state, &headers, params_map).await {
            Ok(order) => Ok(Json(order).into_response()),
            Err(err) => Ok(binance_error(err)),
        }
    } else {
        let params: QueryOrderParams =
            serde_urlencoded::from_str(raw_query.as_deref().unwrap_or_default())
                .map_err(|err| AppError::Validation(format!("invalid query params: {err}")))?;
        let order = if let Some(order_id) = params.order_id {
            state
                .orders_service
                .get_order(params.session_id, order_id)
                .await?
        } else if let Some(client) = params.orig_client_order_id {
            state
                .orders_service
                .get_by_client_id(params.session_id, &client)
                .await?
        } else {
            return Err(AppError::Validation(
                "orderId or origClientOrderId is required".into(),
            ));
        };
        Ok(Json(OrderResponse::from(order)).into_response())
    }
}

#[utoipa::path(
    delete,
    path = "/api/v3/order",
    params(crate::api::v3::orders::types::BinanceQueryParams),
    responses(
        (status = 200, body = crate::dto::v3::orders::BinanceOrderDetails),
        (
            status = 400,
            description = "Validation error",
            body = crate::dto::v3::error::BinanceErrorResponse,
            examples((
                "missingParameter" = (value = json!({
                    "code": -1102,
                    "msg": "Mandatory parameter was not sent, was empty/null, or malformed."
                }))
            ))
        ),
        (
            status = 404,
            description = "Order not found",
            body = crate::dto::v3::error::BinanceErrorResponse,
            examples((
                "orderNotFound" = (value = json!({
                    "code": -2013,
                    "msg": "Order does not exist"
                }))
            ))
        ),
        (
            status = 409,
            description = "Order conflict",
            body = crate::dto::v3::error::BinanceErrorResponse
        ),
        (
            status = 500,
            description = "Internal error",
            body = crate::dto::v3::error::BinanceErrorResponse
        )
    ),
    tag = "binance-v3"
)]
#[instrument(skip(state, raw_query))]
pub async fn cancel_order(
    Extension(state): Extension<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Result<axum::response::Response, AppError> {
    let params_map = parse_query_map(raw_query.as_deref())?;
    if is_binance_request(&params_map) {
        match handle_binance_cancel_order(&state, &headers, params_map).await {
            Ok(order) => Ok(Json(order).into_response()),
            Err(err) => Ok(binance_error(err)),
        }
    } else {
        let params: CancelOrderParams =
            serde_urlencoded::from_str(raw_query.as_deref().unwrap_or_default())
                .map_err(|err| AppError::Validation(format!("invalid query params: {err}")))?;
        let order = if let Some(order_id) = params.order_id {
            state
                .orders_service
                .cancel_order(params.session_id, order_id)
                .await?
        } else if let Some(client) = params.orig_client_order_id {
            let order = state
                .orders_service
                .get_by_client_id(params.session_id, &client)
                .await?;
            state
                .orders_service
                .cancel_order(params.session_id, order.order_id)
                .await?
        } else {
            return Err(AppError::Validation(
                "orderId or origClientOrderId is required".into(),
            ));
        };
        Ok(Json(OrderResponse::from(order)).into_response())
    }
}

#[utoipa::path(
    get,
    path = "/api/v3/openOrders",
    params(crate::api::v3::orders::types::BinanceOpenOrdersParams),
    responses(
        (
            status = 200,
            body = Vec<crate::dto::v3::orders::BinanceOrderDetails>
        ),
        (
            status = 400,
            description = "Validation error",
            body = crate::dto::v3::error::BinanceErrorResponse,
            examples((
                "missingParameter" = (value = json!({
                    "code": -1102,
                    "msg": "Mandatory parameter was not sent, was empty/null, or malformed."
                }))
            ))
        ),
        (
            status = 500,
            description = "Internal error",
            body = crate::dto::v3::error::BinanceErrorResponse
        )
    ),
    tag = "binance-v3"
)]
#[instrument(skip(state, raw_query))]
pub async fn open_orders(
    Extension(state): Extension<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Result<axum::response::Response, AppError> {
    let params_map = parse_query_map(raw_query.as_deref())?;
    if is_binance_request(&params_map) {
        match handle_binance_open_orders(&state, &headers, params_map).await {
            Ok(orders) => Ok(Json(orders).into_response()),
            Err(err) => Ok(binance_error(err)),
        }
    } else {
        let params: OpenOrdersParams =
            serde_urlencoded::from_str(raw_query.as_deref().unwrap_or_default())
                .map_err(|err| AppError::Validation(format!("invalid query params: {err}")))?;
        let orders = state
            .orders_service
            .list_open(params.session_id, params.symbol.as_deref())
            .await?;
        Ok(Json(
            orders
                .into_iter()
                .map(OrderResponse::from)
                .collect::<Vec<_>>(),
        )
        .into_response())
    }
}

#[utoipa::path(
    get,
    path = "/api/v3/myTrades",
    params(crate::api::v3::orders::types::BinanceMyTradesParams),
    responses(
        (
            status = 200,
            body = Vec<crate::dto::v3::trades::BinanceTradeResponse>
        ),
        (
            status = 400,
            description = "Validation error",
            body = crate::dto::v3::error::BinanceErrorResponse,
            examples((
                "missingParameter" = (value = json!({
                    "code": -1102,
                    "msg": "Mandatory parameter was not sent, was empty/null, or malformed."
                }))
            ))
        ),
        (
            status = 500,
            description = "Internal error",
            body = crate::dto::v3::error::BinanceErrorResponse
        )
    ),
    tag = "binance-v3"
)]
#[instrument(skip(state, raw_query))]
pub async fn my_trades(
    Extension(state): Extension<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Result<axum::response::Response, AppError> {
    let params_map = parse_query_map(raw_query.as_deref())?;
    if is_binance_request(&params_map) {
        match handle_binance_my_trades(&state, &headers, params_map).await {
            Ok(trades) => Ok(Json(trades).into_response()),
            Err(err) => Ok(binance_error(err)),
        }
    } else {
        let params: MyTradesParams =
            serde_urlencoded::from_str(raw_query.as_deref().unwrap_or_default())
                .map_err(|err| AppError::Validation(format!("invalid query params: {err}")))?;
        let trades = state
            .orders_service
            .my_trades(params.session_id, &params.symbol)
            .await?;
        Ok(Json(
            trades
                .into_iter()
                .map(FillResponse::from)
                .collect::<Vec<_>>(),
        )
        .into_response())
    }
}
