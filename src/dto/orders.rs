use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::domain::models::{Fill, Liquidity, Order, OrderSide, OrderStatus, OrderType};

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
    pub maker_taker: Option<String>,
}

impl From<Order> for OrderResponse {
    fn from(value: Order) -> Self {
        Self {
            symbol: value.symbol,
            order_id: value.id,
            client_order_id: value.client_order_id,
            order_type: value.order_type,
            side: value.side,
            status: value.status,
            price: value.price.map(|p| p.0),
            orig_qty: value.quantity.0,
            executed_qty: value.filled_quantity.0,
            maker_taker: value.maker_taker.map(|liquidity| match liquidity {
                Liquidity::Maker => "MAKER".to_string(),
                Liquidity::Taker => "TAKER".to_string(),
            }),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FillResponse {
    pub price: f64,
    pub qty: f64,
    pub quote_qty: f64,
    pub commission: f64,
    pub commission_asset: String,
    pub trade_id: i64,
    pub maker: bool,
    pub time: i64,
}

impl From<Fill> for FillResponse {
    fn from(value: Fill) -> Self {
        Self {
            price: value.price.0,
            qty: value.qty.0,
            quote_qty: value.quote_qty,
            commission: value.fee,
            commission_asset: value.fee_asset,
            trade_id: value.trade_id,
            maker: value.maker,
            time: value.event_time.0,
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
