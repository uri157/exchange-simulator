use utoipa::OpenApi;

use crate::dto;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Market
        crate::api::v1::market::exchange_info,
        crate::api::v1::market::local_klines, // GET /api/v1/market/klines (DuckDB)
        crate::api::v1::market::klines,       // GET /api/v3/klines (compat Binance)
        // Datasets (ingesta y consultas locales)
        crate::api::v1::datasets::register_dataset,
        crate::api::v1::datasets::list_datasets,
        crate::api::v1::datasets::ingest_dataset,
        crate::api::v1::datasets::get_ready_symbols,         // GET /api/v1/datasets/symbols
        crate::api::v1::datasets::get_ready_intervals,       // GET /api/v1/datasets/:symbol/intervals
        crate::api::v1::datasets::get_symbol_interval_range, // GET /api/v1/datasets/:symbol/:interval/range
        // Sessions
        crate::api::v1::sessions::create_session,
        crate::api::v1::sessions::list_sessions,
        crate::api::v1::sessions::get_session,
        crate::api::v1::sessions::start_session,
        crate::api::v1::sessions::pause_session,
        crate::api::v1::sessions::resume_session,
        crate::api::v1::sessions::seek_session,
        crate::api::v1::sessions::enable_session,
        crate::api::v1::sessions::disable_session,
        crate::api::v1::sessions::delete_session,
        // Orders
        crate::api::v1::orders::new_order,
        crate::api::v1::orders::get_order,
        crate::api::v1::orders::cancel_order,
        crate::api::v1::orders::open_orders,
        crate::api::v1::orders::my_trades,
        // Account
        crate::api::v1::account::get_account,
        // Binance proxy (opcional)
        crate::api::v1::market_binance::symbols,
        crate::api::v1::market_binance::intervals,
        crate::api::v1::market_binance::range
    ),
    components(
        schemas(
            // DTOs
            dto::market::ExchangeInfoResponse,
            dto::market::SymbolInfo,
            dto::market::KlineResponse,
            dto::market::KlinesParams,
            dto::datasets::CreateDatasetRequest,
            dto::datasets::DatasetResponse,
            dto::datasets::SymbolOnly,
            dto::datasets::IntervalOnly,
            dto::datasets::RangeResponse,
            dto::sessions::CreateSessionRequest,
            dto::sessions::SessionResponse,
            dto::sessions::UpdateSessionEnabledRequest,
            dto::orders::NewOrderRequest,
            dto::orders::NewOrderResponse,
            dto::orders::OrderResponse,
            dto::orders::FillResponse,
            dto::orders::OpenOrdersParams,
            dto::orders::MyTradesParams,
            dto::orders::CancelOrderParams,
            dto::orders::QueryOrderParams,
            dto::account::AccountResponse,
            dto::account::BalanceResponse,
            dto::account::AccountQuery,
            dto::ws::WsQuery,
            // Domain types referenciados por DTOs
            crate::domain::models::OrderStatus,
            crate::domain::models::OrderSide,
            crate::domain::models::OrderType,
            crate::domain::models::SessionStatus,
            crate::domain::models::DatasetFormat,
            crate::domain::value_objects::Interval,
            crate::domain::value_objects::TimestampMs,
            crate::domain::value_objects::Price,
            crate::domain::value_objects::Quantity,
            crate::domain::value_objects::Speed,
            crate::domain::value_objects::DatasetPath
        )
    ),
    tags(
        (name = "market", description = "Market data endpoints (local DuckDB y compat Binance)"),
        (name = "datasets", description = "Dataset ingestion & local dataset queries"),
        (name = "sessions", description = "Replay sessions"),
        (name = "orders", description = "Orders"),
        (name = "account", description = "Accounts"),
        (name = "binance", description = "Binance proxy endpoints")
    )
)]
pub struct ApiDoc;
