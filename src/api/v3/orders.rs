use std::collections::HashMap;

use axum::{
    body::Bytes,
    extract::RawQuery,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use tracing::instrument;
use uuid::Uuid;

use crate::{
    app::bootstrap::AppState,
    domain::models::{OrderSide, OrderStatus, OrderType},
    dto::{
        orders::{
            CancelOrderParams, FillResponse, MyTradesParams, NewOrderRequest, NewOrderResponse,
            OpenOrdersParams, OrderResponse, QueryOrderParams,
        },
        v3::{
            error::BinanceErrorResponse,
            orders::{
                BinanceNewOrderResponse, BinanceOrderDetails, BinanceOrderFill, BinanceOrderSide,
                BinanceOrderType, BinanceTimeInForce, NewOrderRespType,
            },
            trades::BinanceTradeResponse,
        },
    },
    error::AppError,
};

use super::account::symbol_components;

const ORDER_LIST_ID_NONE: i64 = -1;

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/v3/order",
            post(new_order).get(get_order).delete(cancel_order),
        )
        .route("/api/v3/openOrders", get(open_orders))
        .route("/api/v3/myTrades", get(my_trades))
}

enum NewOrderPayload {
    Legacy(NewOrderRequest),
    Binance(BinanceNewOrderParams),
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceNewOrderParams {
    symbol: Option<String>,
    side: Option<String>,
    #[serde(rename = "type")]
    order_type: Option<String>,
    time_in_force: Option<String>,
    quantity: Option<String>,
    quote_order_qty: Option<String>,
    price: Option<String>,
    timestamp: Option<String>,
    recv_window: Option<String>,
    new_client_order_id: Option<String>,
    new_order_resp_type: Option<String>,
    session_id: Option<String>,
    signature: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceQueryParams {
    symbol: Option<String>,
    order_id: Option<String>,
    orig_client_order_id: Option<String>,
    timestamp: Option<String>,
    recv_window: Option<String>,
    session_id: Option<String>,
    signature: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceOpenOrdersParams {
    symbol: Option<String>,
    timestamp: Option<String>,
    recv_window: Option<String>,
    session_id: Option<String>,
    signature: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceMyTradesParams {
    symbol: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    from_id: Option<String>,
    limit: Option<String>,
    timestamp: Option<String>,
    recv_window: Option<String>,
    session_id: Option<String>,
    signature: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v3/order",
    request_body = crate::dto::orders::NewOrderRequest,
    responses((status = 200, body = crate::dto::orders::NewOrderResponse))
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
    params(crate::dto::orders::QueryOrderParams),
    responses((status = 200, body = crate::dto::orders::OrderResponse))
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
    params(crate::dto::orders::CancelOrderParams),
    responses((status = 200, body = crate::dto::orders::OrderResponse))
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
    params(crate::dto::orders::OpenOrdersParams),
    responses((status = 200, body = Vec<crate::dto::orders::OrderResponse>))
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
    params(crate::dto::orders::MyTradesParams),
    responses((status = 200, body = Vec<crate::dto::orders::FillResponse>))
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

fn parse_new_order_payload(
    headers: &HeaderMap,
    raw_query: Option<&str>,
    body: &[u8],
) -> Result<NewOrderPayload, AppError> {
    let is_json = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase())
        .map(|v| v.starts_with("application/json"))
        .unwrap_or(false);

    if is_json {
        let payload: NewOrderRequest = serde_json::from_slice(body)
            .map_err(|err| AppError::Validation(format!("invalid JSON payload: {err}")))?;
        return Ok(NewOrderPayload::Legacy(payload));
    }

    let mut params = parse_form_map(body)?;
    if let Some(query) = raw_query {
        params.extend(parse_query_map(Some(query))?);
    }

    let params: BinanceNewOrderParams = map_to_struct(params)?;
    Ok(NewOrderPayload::Binance(params))
}

fn parse_form_map(body: &[u8]) -> Result<HashMap<String, String>, AppError> {
    if body.is_empty() {
        return Ok(HashMap::new());
    }
    let pairs: Vec<(String, String)> = serde_urlencoded::from_bytes(body)
        .map_err(|err| AppError::Validation(format!("invalid form payload: {err}")))?;
    Ok(pairs.into_iter().collect())
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

fn map_to_struct<T>(map: HashMap<String, String>) -> Result<T, AppError>
where
    T: for<'de> Deserialize<'de>,
{
    let value = serde_json::to_value(map)
        .map_err(|err| AppError::Validation(format!("failed to convert params: {err}")))?;
    serde_json::from_value::<T>(convert_map_keys(value))
        .map_err(|err| AppError::Validation(format!("invalid parameters: {err}")))
}

fn convert_map_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mapped = map
                .into_iter()
                .map(|(k, v)| (k, convert_map_keys(v)))
                .collect();
            Value::Object(mapped)
        }
        other => other,
    }
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

fn is_binance_request(params: &HashMap<String, String>) -> bool {
    params.contains_key("timestamp")
        || params.contains_key("recvWindow")
        || params
            .get("orderId")
            .map(|value| value.parse::<Uuid>().is_err())
            .unwrap_or(false)
}

async fn handle_binance_new_order(
    state: &AppState,
    headers: &HeaderMap,
    _raw_query: Option<&str>,
    params: BinanceNewOrderParams,
) -> Result<BinanceNewOrderResponse, AppError> {
    let session_id = extract_session_id(headers, params.session_id.as_deref())?;
    let symbol_param = required(&params.symbol, "symbol")?;
    let symbol = symbol_param.to_string();
    let side = parse_side(required(&params.side, "side")?)?;
    let order_type = parse_order_type(required(&params.order_type, "type")?)?;
    let resp_type = params
        .new_order_resp_type
        .as_deref()
        .map(parse_resp_type)
        .transpose()?
        .unwrap_or_default();

    let mut quantity = None;
    let mut price = None;
    match order_type {
        BinanceOrderType::Limit => {
            let tif = params.time_in_force.as_deref().ok_or_else(|| {
                AppError::Validation("timeInForce is required for LIMIT orders".into())
            })?;
            let tif = parse_time_in_force(tif)?;
            price = Some(parse_decimal(required(&params.price, "price")?)?);
            quantity = Some(parse_decimal(required(&params.quantity, "quantity")?)?);
            // ensure time_in_force is valid (currently only GTC supported)
            if !matches!(tif, BinanceTimeInForce::Gtc) {
                return Err(AppError::Validation(
                    "only GTC timeInForce is supported".into(),
                ));
            }
        }
        BinanceOrderType::Market => {
            match (
                params.quantity.as_deref(),
                params.quote_order_qty.as_deref(),
            ) {
                (Some(qty), None) => {
                    quantity = Some(parse_decimal(qty)?);
                }
                (None, Some(quote_qty)) => {
                    let quote = parse_decimal(quote_qty)?;
                    let latest = state
                        .replay_service
                        .latest_kline(session_id, &symbol)
                        .await?
                        .ok_or_else(|| {
                            AppError::Validation("no market data for session yet".into())
                        })?;
                    if latest.close.0 == 0.0 {
                        return Err(AppError::Validation(
                            "cannot determine quantity for quoteOrderQty".into(),
                        ));
                    }
                    quantity = Some(quote / latest.close.0);
                }
                (Some(_), Some(_)) => {
                    return Err(AppError::Validation(
                        "provide either quantity or quoteOrderQty for MARKET orders".into(),
                    ));
                }
                (None, None) => {
                    return Err(AppError::Validation(
                        "quantity or quoteOrderQty is required for MARKET orders".into(),
                    ));
                }
            }
        }
    }

    let quantity =
        quantity.ok_or_else(|| AppError::Validation("quantity could not be determined".into()))?;

    let client_order_id = params.new_client_order_id.clone();
    let (order, fills) = state
        .orders_service
        .place_order(
            session_id,
            symbol.clone(),
            match side {
                BinanceOrderSide::Buy => OrderSide::Buy,
                BinanceOrderSide::Sell => OrderSide::Sell,
            },
            match order_type {
                BinanceOrderType::Market => OrderType::Market,
                BinanceOrderType::Limit => OrderType::Limit,
            },
            crate::domain::value_objects::Quantity(quantity),
            price.map(crate::domain::value_objects::Price),
            client_order_id.clone(),
        )
        .await?;

    let numeric_id = state
        .order_id_mapping
        .ensure_mapping(session_id, order.order_id)
        .await;

    let fills_for_response: Vec<BinanceOrderFill> = fills
        .iter()
        .enumerate()
        .map(|(idx, fill)| {
            let (_, quote) =
                symbol_components(&fill.symbol, state.account_service.default_quote_asset());
            BinanceOrderFill {
                price: format_decimal(fill.price.0),
                qty: format_decimal(fill.quantity.0),
                commission: format_decimal(fill.fee.0),
                commission_asset: quote,
                trade_id: idx as u64,
            }
        })
        .collect();

    let executed_qty = order.filled_quantity.0;
    let cumm_quote: f64 = fills.iter().map(|f| f.price.0 * f.quantity.0).sum();

    let mut response = BinanceNewOrderResponse {
        symbol: symbol.clone(),
        order_id: numeric_id,
        order_list_id: ORDER_LIST_ID_NONE,
        client_order_id,
        transact_time: order.created_at.0,
        price: None,
        orig_qty: None,
        executed_qty: None,
        cummulative_quote_qty: None,
        status: None,
        time_in_force: None,
        order_type: None,
        side: None,
        fills: None,
    };

    if matches!(resp_type, NewOrderRespType::Result | NewOrderRespType::Full) {
        response.price = Some(format_decimal(order.price.map(|p| p.0).unwrap_or(0.0)));
        response.orig_qty = Some(format_decimal(order.quantity.0));
        response.executed_qty = Some(format_decimal(executed_qty));
        response.cummulative_quote_qty = Some(format_decimal(cumm_quote));
        response.status = Some(format_status(order.status));
        response.time_in_force = Some("GTC".to_string());
        response.order_type = Some(format_order_type(order.order_type));
        response.side = Some(format_side(order.side));
    }

    if matches!(resp_type, NewOrderRespType::Full) {
        response.fills = Some(fills_for_response);
    }

    Ok(response)
}

async fn handle_binance_get_order(
    state: &AppState,
    headers: &HeaderMap,
    params_map: HashMap<String, String>,
) -> Result<BinanceOrderDetails, AppError> {
    let params: BinanceQueryParams = map_to_struct(params_map)?;
    let session_id = extract_session_id(headers, params.session_id.as_deref())?;
    let order = resolve_order(
        state,
        session_id,
        params.order_id.as_deref(),
        params.orig_client_order_id.as_deref(),
    )
    .await?;
    build_order_details(state, session_id, order).await
}

async fn handle_binance_cancel_order(
    state: &AppState,
    headers: &HeaderMap,
    params_map: HashMap<String, String>,
) -> Result<BinanceOrderDetails, AppError> {
    let params: BinanceQueryParams = map_to_struct(params_map)?;
    let session_id = extract_session_id(headers, params.session_id.as_deref())?;
    let order = if let Some(order_id) = params.order_id.as_deref() {
        let numeric = order_id
            .parse::<u64>()
            .map_err(|_| AppError::Validation("invalid orderId".into()))?;
        let uuid = state
            .order_id_mapping
            .resolve_uuid(session_id, numeric)
            .await
            .ok_or_else(|| AppError::NotFound("order not found".into()))?;
        state.orders_service.cancel_order(session_id, uuid).await?
    } else if let Some(client) = params.orig_client_order_id.as_deref() {
        let order = state
            .orders_service
            .get_by_client_id(session_id, client)
            .await?;
        state
            .orders_service
            .cancel_order(session_id, order.order_id)
            .await?
    } else {
        return Err(AppError::Validation(
            "orderId or origClientOrderId is required".into(),
        ));
    };
    state
        .order_id_mapping
        .ensure_mapping(session_id, order.order_id)
        .await;
    build_order_details(state, session_id, order).await
}

async fn handle_binance_open_orders(
    state: &AppState,
    headers: &HeaderMap,
    params_map: HashMap<String, String>,
) -> Result<Vec<BinanceOrderDetails>, AppError> {
    let params: BinanceOpenOrdersParams = map_to_struct(params_map)?;
    let session_id = extract_session_id(headers, params.session_id.as_deref())?;
    let orders = state
        .orders_service
        .list_open(session_id, params.symbol.as_deref())
        .await?;
    let mut details = Vec::new();
    for order in orders {
        state
            .order_id_mapping
            .ensure_mapping(session_id, order.order_id)
            .await;
        details.push(build_order_details(state, session_id, order).await?);
    }
    Ok(details)
}

async fn handle_binance_my_trades(
    state: &AppState,
    headers: &HeaderMap,
    params_map: HashMap<String, String>,
) -> Result<Vec<BinanceTradeResponse>, AppError> {
    let params: BinanceMyTradesParams = map_to_struct(params_map)?;
    let session_id = extract_session_id(headers, params.session_id.as_deref())?;
    let symbol = required(&params.symbol, "symbol")?;
    let fills = state.orders_service.my_trades(session_id, symbol).await?;
    let mut trades = Vec::new();
    for (idx, fill) in fills.into_iter().enumerate() {
        let order = state
            .orders_service
            .get_order(session_id, fill.order_id)
            .await?;
        let numeric_id = state
            .order_id_mapping
            .ensure_mapping(session_id, order.order_id)
            .await;
        let (_, quote) =
            symbol_components(&order.symbol, state.account_service.default_quote_asset());
        trades.push(BinanceTradeResponse {
            symbol: order.symbol.clone(),
            id: idx as u64,
            order_id: numeric_id,
            order_list_id: ORDER_LIST_ID_NONE,
            price: format_decimal(fill.price.0),
            qty: format_decimal(fill.quantity.0),
            quote_qty: format_decimal(fill.price.0 * fill.quantity.0),
            commission: format_decimal(fill.fee.0),
            commission_asset: quote,
            time: fill.trade_time.0,
            is_buyer: matches!(order.side, OrderSide::Buy),
            is_maker: false,
            is_best_match: true,
        });
    }
    Ok(trades)
}

async fn resolve_order(
    state: &AppState,
    session_id: Uuid,
    order_id: Option<&str>,
    client_id: Option<&str>,
) -> Result<crate::domain::models::Order, AppError> {
    if let Some(order_id) = order_id {
        let numeric = order_id
            .parse::<u64>()
            .map_err(|_| AppError::Validation("invalid orderId".into()))?;
        let uuid = state
            .order_id_mapping
            .resolve_uuid(session_id, numeric)
            .await
            .ok_or_else(|| AppError::NotFound("order not found".into()))?;
        state.orders_service.get_order(session_id, uuid).await
    } else if let Some(client) = client_id {
        state
            .orders_service
            .get_by_client_id(session_id, client)
            .await
    } else {
        Err(AppError::Validation(
            "orderId or origClientOrderId is required".into(),
        ))
    }
}

async fn build_order_details(
    state: &AppState,
    session_id: Uuid,
    order: crate::domain::models::Order,
) -> Result<BinanceOrderDetails, AppError> {
    let numeric_id = state
        .order_id_mapping
        .ensure_mapping(session_id, order.order_id)
        .await;
    let fills = state
        .orders_service
        .my_trades(session_id, &order.symbol)
        .await?;
    let relevant_fills: Vec<_> = fills
        .into_iter()
        .filter(|fill| fill.order_id == order.order_id)
        .collect();
    let cumm_quote: f64 = relevant_fills
        .iter()
        .map(|f| f.price.0 * f.quantity.0)
        .sum();

    let status = order.status.clone();
    let side = order.side.clone();
    let order_type = order.order_type.clone();

    Ok(BinanceOrderDetails {
        symbol: order.symbol.clone(),
        order_id: numeric_id,
        order_list_id: ORDER_LIST_ID_NONE,
        client_order_id: order.client_order_id.clone(),
        price: format_decimal(order.price.map(|p| p.0).unwrap_or(0.0)),
        orig_qty: format_decimal(order.quantity.0),
        executed_qty: format_decimal(order.filled_quantity.0),
        cummulative_quote_qty: format_decimal(cumm_quote),
        status: format_status(status.clone()),
        time_in_force: "GTC".to_string(),
        order_type: format_order_type(order_type),
        side: format_side(side.clone()),
        stop_price: format_decimal(0.0),
        iceberg_qty: format_decimal(0.0),
        time: order.created_at.0,
        update_time: order.created_at.0,
        is_working: !matches!(status, OrderStatus::Canceled),
        working_time: order.created_at.0,
        orig_quote_order_qty: format_decimal(
            order.price.map(|p| p.0 * order.quantity.0).unwrap_or(0.0),
        ),
    })
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

fn parse_side(value: &str) -> Result<BinanceOrderSide, AppError> {
    serde_json::from_str(&format!("\"{}\"", value.to_ascii_uppercase()))
        .map_err(|_| AppError::Validation("invalid side".into()))
}

fn parse_order_type(value: &str) -> Result<BinanceOrderType, AppError> {
    serde_json::from_str(&format!("\"{}\"", value.to_ascii_uppercase()))
        .map_err(|_| AppError::Validation("invalid type".into()))
}

fn parse_time_in_force(value: &str) -> Result<BinanceTimeInForce, AppError> {
    serde_json::from_str(&format!("\"{}\"", value.to_ascii_uppercase()))
        .map_err(|_| AppError::Validation("unsupported timeInForce".into()))
}

fn parse_resp_type(value: &str) -> Result<NewOrderRespType, AppError> {
    serde_json::from_str(&format!("\"{}\"", value.to_ascii_uppercase()))
        .map_err(|_| AppError::Validation("invalid newOrderRespType".into()))
}

fn parse_decimal(value: &str) -> Result<f64, AppError> {
    value
        .parse::<f64>()
        .map_err(|_| AppError::Validation(format!("invalid decimal: {value}")))
}

fn required<'a>(value: &'a Option<String>, name: &str) -> Result<&'a str, AppError> {
    value
        .as_deref()
        .ok_or_else(|| AppError::Validation(format!("{name} is required")))
}

fn format_decimal(value: f64) -> String {
    format!("{:.8}", value)
}

fn format_status(status: OrderStatus) -> String {
    match status {
        OrderStatus::New => "NEW".to_string(),
        OrderStatus::Filled => "FILLED".to_string(),
        OrderStatus::PartiallyFilled => "PARTIALLY_FILLED".to_string(),
        OrderStatus::Canceled => "CANCELED".to_string(),
    }
}

fn format_order_type(order_type: OrderType) -> String {
    match order_type {
        OrderType::Market => "MARKET".to_string(),
        OrderType::Limit => "LIMIT".to_string(),
    }
}

fn format_side(side: OrderSide) -> String {
    match side {
        OrderSide::Buy => "BUY".to_string(),
        OrderSide::Sell => "SELL".to_string(),
    }
}
