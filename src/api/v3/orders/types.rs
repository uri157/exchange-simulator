#![allow(dead_code)]

use serde::Deserialize;

use crate::dto::orders::NewOrderRequest;

pub const ORDER_LIST_ID_NONE: i64 = -1;

pub enum NewOrderPayload {
    Legacy(NewOrderRequest),
    Binance(BinanceNewOrderParams),
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceNewOrderParams {
    pub symbol: Option<String>,
    pub side: Option<String>,
    #[serde(rename = "type")]
    pub order_type: Option<String>,
    pub time_in_force: Option<String>,
    pub quantity: Option<String>,
    pub quote_order_qty: Option<String>,
    pub price: Option<String>,
    pub timestamp: Option<String>,
    pub recv_window: Option<String>,
    pub new_client_order_id: Option<String>,
    pub new_order_resp_type: Option<String>,
    pub session_id: Option<String>,
    pub signature: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceQueryParams {
    pub symbol: Option<String>,
    pub order_id: Option<String>,
    pub orig_client_order_id: Option<String>,
    pub timestamp: Option<String>,
    pub recv_window: Option<String>,
    pub session_id: Option<String>,
    pub signature: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOpenOrdersParams {
    pub symbol: Option<String>,
    pub timestamp: Option<String>,
    pub recv_window: Option<String>,
    pub session_id: Option<String>,
    pub signature: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceMyTradesParams {
    pub symbol: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub from_id: Option<String>,
    pub limit: Option<String>,
    pub timestamp: Option<String>,
    pub recv_window: Option<String>,
    pub session_id: Option<String>,
    pub signature: Option<String>,
}
