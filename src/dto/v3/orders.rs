use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum BinanceOrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum BinanceOrderType {
    Market,
    Limit,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum BinanceTimeInForce {
    Gtc,
}

impl BinanceTimeInForce {
    pub fn as_str(&self) -> &'static str {
        match self {
            BinanceTimeInForce::Gtc => "GTC",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum NewOrderRespType {
    Ack,
    Result,
    Full,
}

impl Default for NewOrderRespType {
    fn default() -> Self {
        NewOrderRespType::Ack
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BinanceNewOrderResponse {
    pub symbol: String,
    #[schema(value_type = i64, format = Int64)]
    pub order_id: u64,
    #[schema(value_type = i64, format = Int64)]
    pub order_list_id: i64,
    pub client_order_id: Option<String>,
    pub transact_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cummulative_quote_qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub order_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fills: Option<Vec<BinanceOrderFill>>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOrderFill {
    pub price: String,
    pub qty: String,
    pub commission: String,
    pub commission_asset: String,
    pub trade_id: u64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BinanceOrderDetails {
    pub symbol: String,
    #[schema(value_type = i64, format = Int64)]
    pub order_id: u64,
    pub order_list_id: i64,
    pub client_order_id: Option<String>,
    pub price: String,
    pub orig_qty: String,
    pub executed_qty: String,
    pub cummulative_quote_qty: String,
    pub status: String,
    pub time_in_force: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub side: String,
    pub stop_price: String,
    pub iceberg_qty: String,
    pub time: i64,
    pub update_time: i64,
    pub is_working: bool,
    pub working_time: i64,
    pub orig_quote_order_qty: String,
}
