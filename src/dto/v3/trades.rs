use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BinanceTradeResponse {
    pub symbol: String,
    pub id: u64,
    #[schema(value_type = i64, format = Int64)]
    pub order_id: u64,
    #[schema(value_type = i64, format = Int64)]
    pub order_list_id: i64,
    pub price: String,
    pub qty: String,
    pub quote_qty: String,
    pub commission: String,
    pub commission_asset: String,
    pub time: i64,
    pub is_buyer: bool,
    pub is_maker: bool,
    pub is_best_match: bool,
}
