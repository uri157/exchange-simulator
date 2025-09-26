#![allow(dead_code)]

use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::dto::orders::NewOrderRequest;

pub const ORDER_LIST_ID_NONE: i64 = -1;

pub enum NewOrderPayload {
    Legacy(NewOrderRequest),
    Binance(BinanceNewOrderParams),
}

#[derive(Debug, Default, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query, rename_all = "camelCase")]
pub struct BinanceNewOrderParams {
    /// Trading pair symbol, e.g. ETHBTC.
    #[param(required = true)]
    #[schema(required = true, example = "ETHBTC")]
    pub symbol: Option<String>,
    /// Order side. Allowed values: BUY or SELL.
    #[param(required = true)]
    #[schema(required = true, example = "BUY")]
    pub side: Option<String>,
    #[serde(rename = "type")]
    /// Order type. LIMIT requires timeInForce, price and quantity. MARKET requires either quantity or quoteOrderQty.
    #[param(required = true)]
    #[schema(required = true, example = "LIMIT")]
    pub order_type: Option<String>,
    /// Required when type=LIMIT. Only GTC is supported.
    pub time_in_force: Option<String>,
    /// Required for LIMIT orders and for MARKET when quoteOrderQty is not provided.
    pub quantity: Option<String>,
    /// Mutually exclusive with quantity for MARKET orders. Total quote amount to trade.
    pub quote_order_qty: Option<String>,
    /// Required for LIMIT orders. Price per unit.
    pub price: Option<String>,
    /// Optional timestamp in milliseconds for Binance compatibility.
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub timestamp: Option<String>,
    /// Optional recvWindow in milliseconds. Ignored if present.
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub recv_window: Option<String>,
    /// Optional client-defined order identifier.
    pub new_client_order_id: Option<String>,
    /// Response type hint. Supported values: ACK, RESULT, FULL.
    pub new_order_resp_type: Option<String>,
    /// Optional session identifier. If omitted, X-Session-Id header is used.
    pub session_id: Option<String>,
    /// Signature parameter is accepted for compatibility but ignored.
    pub signature: Option<String>,
}

#[derive(Debug, Default, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query, rename_all = "camelCase")]
pub struct BinanceQueryParams {
    /// Trading pair symbol. Required when querying by symbol only endpoints.
    pub symbol: Option<String>,
    /// Numeric identifier of the order (int64).
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub order_id: Option<String>,
    /// Original client order identifier. Provide either orderId or origClientOrderId.
    pub orig_client_order_id: Option<String>,
    /// Optional timestamp in milliseconds for Binance compatibility.
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub timestamp: Option<String>,
    /// Optional recvWindow in milliseconds. Ignored if present.
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub recv_window: Option<String>,
    /// Optional session identifier. If omitted, X-Session-Id header is used.
    pub session_id: Option<String>,
    /// Signature parameter is accepted for compatibility but ignored.
    pub signature: Option<String>,
}

#[derive(Debug, Default, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query, rename_all = "camelCase")]
pub struct BinanceOpenOrdersParams {
    /// Optional trading pair symbol. When omitted, all open orders are returned.
    pub symbol: Option<String>,
    /// Optional timestamp in milliseconds for Binance compatibility.
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub timestamp: Option<String>,
    /// Optional recvWindow in milliseconds. Ignored if present.
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub recv_window: Option<String>,
    /// Optional session identifier. If omitted, X-Session-Id header is used.
    pub session_id: Option<String>,
    /// Signature parameter is accepted for compatibility but ignored.
    pub signature: Option<String>,
}

#[derive(Debug, Default, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query, rename_all = "camelCase")]
pub struct BinanceMyTradesParams {
    /// Trading pair symbol to query trades for.
    #[param(required = true)]
    #[schema(required = true)]
    pub symbol: Option<String>,
    /// Optional start time in milliseconds.
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub start_time: Option<String>,
    /// Optional end time in milliseconds.
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub end_time: Option<String>,
    /// Optional trade id to fetch from.
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub from_id: Option<String>,
    /// Optional max number of trades to return (default 500, max 1000).
    pub limit: Option<String>,
    /// Optional timestamp in milliseconds for Binance compatibility.
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub timestamp: Option<String>,
    /// Optional recvWindow in milliseconds. Ignored if present.
    #[param(value_type = i64, format = Int64)]
    #[schema(value_type = i64, format = Int64)]
    pub recv_window: Option<String>,
    /// Optional session identifier. If omitted, X-Session-Id header is used.
    pub session_id: Option<String>,
    /// Signature parameter is accepted for compatibility but ignored.
    pub signature: Option<String>,
}
