use utoipa::OpenApi;

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Exchange Simulator — Binance v3 (Bots)",
        description = "Spec para endpoints compatibles con Binance usados por bots (ordenes, cuenta, trades)."
    ),
    paths(
        crate::api::v3::orders::handlers::new_order,
        crate::api::v3::orders::handlers::get_order,
        crate::api::v3::orders::handlers::cancel_order,
        crate::api::v3::orders::handlers::open_orders,
        crate::api::v3::orders::handlers::my_trades,
        crate::api::v3::account::get_account
    ),
    components(schemas(
        crate::api::v3::orders::types::BinanceNewOrderParams,
        crate::api::v3::orders::types::BinanceQueryParams,
        crate::api::v3::orders::types::BinanceOpenOrdersParams,
        crate::api::v3::orders::types::BinanceMyTradesParams,
        crate::api::v3::account::BinanceAccountParams,
        crate::dto::v3::orders::BinanceNewOrderResponse,
        crate::dto::v3::orders::BinanceOrderDetails,
        crate::dto::v3::orders::BinanceOrderFill,
        crate::dto::v3::orders::BinanceOrderSide,
        crate::dto::v3::orders::BinanceOrderType,
        crate::dto::v3::orders::BinanceTimeInForce,
        crate::dto::v3::orders::NewOrderRespType,
        crate::dto::v3::trades::BinanceTradeResponse,
        crate::dto::v3::account::BinanceAccountResponse,
        crate::dto::v3::account::BinanceBalance,
        crate::dto::v3::error::BinanceErrorResponse
    )),
    tags((name = "binance-v3", description = "Endpoints Binance-like para bots")),
    modifiers(&BinanceV3DocModifier)
)]
pub struct BinanceV3Api;

struct BinanceV3DocModifier;

impl utoipa::Modify for BinanceV3DocModifier {
    fn modify(&self, doc: &mut utoipa::openapi::OpenApi) {
        doc.info.version = PKG_VERSION.to_string();
    }
}
