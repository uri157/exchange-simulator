use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::domain::models::{Fill, Order, OrderSide, OrderStatus, OrderType};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewOrderRequest {
    pub session_id: Uuid,
    pub symbol: String,
    pub side: OrderSide,
    #[serde(rename = "type")]
    pub order_type: OrderType,
    pub quantity: f64,
    pub price: Option<f64>,
    pub client_order_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
    pub symbol: String,
    pub order_id: Uuid,
    pub client_order_id: Option<String>,
    #[serde(rename = "type")]
    pub order_type: OrderType,
    pub side: OrderSide,
    pub status: OrderStatus,
    pub price: Option<f64>,
    pub orig_qty: f64,
    pub executed_qty: f64,
}

impl From<Order> for OrderResponse {
    fn from(value: Order) -> Self {
        Self {
            symbol: value.symbol,
            order_id: value.order_id,
            client_order_id: value.client_order_id,
            order_type: value.order_type,
            side: value.side,
            status: value.status,
            price: value.price.map(|p| p.0),
            orig_qty: value.quantity.0,
            executed_qty: value.filled_quantity.0,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FillResponse {
    pub price: f64,
    pub qty: f64,
    pub commission: f64,
    pub time: i64,
}

impl From<Fill> for FillResponse {
    fn from(value: Fill) -> Self {
        Self {
            price: value.price.0,
            qty: value.quantity.0,
            commission: value.fee.0,
            time: value.trade_time.0,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewOrderResponse {
    #[serde(flatten)]
    pub order: OrderResponse,
    pub fills: Vec<FillResponse>,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryOrderParams {
    pub session_id: Uuid,
    pub symbol: Option<String>,
    pub order_id: Option<Uuid>,
    pub orig_client_order_id: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderParams {
    pub session_id: Uuid,
    pub symbol: String,
    pub order_id: Option<Uuid>,
    pub orig_client_order_id: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenOrdersParams {
    pub session_id: Uuid,
    pub symbol: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MyTradesParams {
    pub session_id: Uuid,
    pub symbol: String,
}
