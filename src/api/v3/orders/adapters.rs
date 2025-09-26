use std::collections::HashMap;

use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    app::bootstrap::AppState,
    domain::models::{OrderSide, OrderType},
    dto::{
        orders::NewOrderRequest,
        v3::{
            orders::{
                BinanceNewOrderResponse, BinanceOrderDetails, BinanceOrderFill, BinanceOrderSide,
                BinanceOrderType, BinanceTimeInForce, NewOrderRespType,
            },
            trades::BinanceTradeResponse,
        },
    },
    error::AppError,
};

use super::{
    super::account::symbol_components,
    mappers::{build_order_details, format_decimal, format_order_type, format_side, format_status},
    types::{
        BinanceMyTradesParams, BinanceNewOrderParams, BinanceOpenOrdersParams, BinanceQueryParams,
        NewOrderPayload, ORDER_LIST_ID_NONE,
    },
    validators::{
        extract_session_id, parse_decimal, parse_order_type, parse_resp_type, parse_side,
        parse_time_in_force, required,
    },
};

pub fn parse_new_order_payload(
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

pub fn parse_query_map(raw_query: Option<&str>) -> Result<HashMap<String, String>, AppError> {
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

pub fn is_binance_request(params: &HashMap<String, String>) -> bool {
    params.contains_key("timestamp")
        || params.contains_key("recvWindow")
        || params
            .get("orderId")
            .map(|value| value.parse::<Uuid>().is_err())
            .unwrap_or(false)
}

pub async fn handle_binance_new_order(
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

    let (quantity, price) = match order_type {
        BinanceOrderType::Limit => {
            let tif = params.time_in_force.as_deref().ok_or_else(|| {
                AppError::Validation("timeInForce is required for LIMIT orders".into())
            })?;
            let tif = parse_time_in_force(tif)?;
            let price = parse_decimal(required(&params.price, "price")?)?;
            let quantity = parse_decimal(required(&params.quantity, "quantity")?)?;
            if !matches!(tif, BinanceTimeInForce::Gtc) {
                return Err(AppError::Validation(
                    "only GTC timeInForce is supported".into(),
                ));
            }
            (quantity, Some(price))
        }
        BinanceOrderType::Market => {
            let quantity = match (
                params.quantity.as_deref(),
                params.quote_order_qty.as_deref(),
            ) {
                (Some(qty), None) => parse_decimal(qty)?,
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
                    quote / latest.close.0
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
            };
            (quantity, None)
        }
    };

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

pub async fn handle_binance_get_order(
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

pub async fn handle_binance_cancel_order(
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

pub async fn handle_binance_open_orders(
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

pub async fn handle_binance_my_trades(
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

fn parse_form_map(body: &[u8]) -> Result<HashMap<String, String>, AppError> {
    if body.is_empty() {
        return Ok(HashMap::new());
    }
    let pairs: Vec<(String, String)> = serde_urlencoded::from_bytes(body)
        .map_err(|err| AppError::Validation(format!("invalid form payload: {err}")))?;
    Ok(pairs.into_iter().collect())
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
