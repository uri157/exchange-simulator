use utoipa::OpenApi;

use crate::dto;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::v1::market::exchange_info,
        crate::api::v1::market::klines,
        crate::api::v1::datasets::register_dataset,
        crate::api::v1::datasets::list_datasets,
        crate::api::v1::datasets::ingest_dataset,
        crate::api::v1::sessions::create_session,
        crate::api::v1::sessions::list_sessions,
        crate::api::v1::sessions::get_session,
        crate::api::v1::sessions::start_session,
        crate::api::v1::sessions::pause_session,
        crate::api::v1::sessions::resume_session,
        crate::api::v1::sessions::seek_session,
        crate::api::v1::orders::new_order,
        crate::api::v1::orders::get_order,
        crate::api::v1::orders::cancel_order,
        crate::api::v1::orders::open_orders,
        crate::api::v1::orders::my_trades,
        crate::api::v1::account::get_account,
    ),
    components(schemas(
        dto::market::ExchangeInfoResponse,
        dto::market::SymbolInfo,
        dto::market::KlineResponse,
        dto::datasets::RegisterDatasetRequest,
        dto::datasets::DatasetResponse,
        dto::sessions::CreateSessionRequest,
        dto::sessions::SessionResponse,
        dto::orders::NewOrderRequest,
        dto::orders::NewOrderResponse,
        dto::orders::OrderResponse,
        dto::orders::FillResponse,
        dto::orders::OpenOrdersParams,
        dto::orders::MyTradesParams,
        dto::account::AccountResponse,
        dto::account::BalanceResponse,
        dto::account::AccountQuery,
        dto::ws::WsQuery,
    )),
    tags(
        (name = "market", description = "Market data endpoints"),
        (name = "datasets", description = "Dataset ingestion"),
        (name = "sessions", description = "Replay sessions"),
        (name = "orders", description = "Orders"),
        (name = "account", description = "Accounts"),
    )
)]
pub struct ApiDoc;
