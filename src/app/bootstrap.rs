use std::sync::Arc;

use axum::Router;

use crate::{
    app::router::create_router,
    domain::traits::{AccountsRepo, MarketIngestor, MarketStore, OrdersRepo, SessionsRepo},
    infra::{
        clock::SimulatedClock,
        config::AppConfig,
        duckdb::{db::DuckDbPool, ingest_repo::DuckDbIngestRepo, market_repo::DuckDbMarketStore},
        repos::memory::{MemoryAccountsRepo, MemoryOrdersRepo, MemorySessionsRepo},
        ws::broadcaster::SessionBroadcaster,
    },
    services::{
        account_service::AccountService, ingest_service::IngestService,
        market_service::MarketService, orders_service::OrdersService,
        replay_service::ReplayService, sessions_service::SessionsService,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub market_service: Arc<MarketService>,
    pub ingest_service: Arc<IngestService>,
    pub sessions_service: Arc<SessionsService>,
    pub orders_service: Arc<OrdersService>,
    pub account_service: Arc<AccountService>,
    pub replay_service: Arc<ReplayService>,
    pub broadcaster: SessionBroadcaster,
}

pub fn build_app(config: AppConfig) -> Result<Router, crate::error::AppError> {
    let pool = DuckDbPool::new(&config.duckdb_path)?;

    let market_store: Arc<dyn MarketStore> = Arc::new(DuckDbMarketStore::new(pool.clone()));
    let market_service = Arc::new(MarketService::new(market_store.clone()));

    let ingestor: Arc<dyn MarketIngestor> = Arc::new(DuckDbIngestRepo::new(pool.clone()));
    let ingest_service = Arc::new(IngestService::new(ingestor.clone()));

    let sessions_repo: Arc<dyn SessionsRepo> = Arc::new(MemorySessionsRepo::new());
    let orders_repo: Arc<dyn OrdersRepo> = Arc::new(MemoryOrdersRepo::new());
    let accounts_repo: Arc<dyn AccountsRepo> = Arc::new(MemoryAccountsRepo::new());

    let clock = Arc::new(SimulatedClock::new(config.default_speed));
    let clock_trait: Arc<dyn crate::domain::traits::Clock> = clock.clone();

    let broadcaster = SessionBroadcaster::new(config.ws_buffer);

    let replay_service = Arc::new(ReplayService::new(
        market_store.clone(),
        clock_trait.clone(),
        sessions_repo.clone(),
        broadcaster.clone(),
    ));

    let replay_engine: Arc<dyn crate::domain::traits::ReplayEngine> = replay_service.clone();

    let account_service = Arc::new(AccountService::new(
        accounts_repo.clone(),
        "USDT".to_string(),
        10_000.0,
    ));

    let orders_service = Arc::new(OrdersService::new(
        orders_repo.clone(),
        sessions_repo.clone(),
        account_service.clone(),
        replay_service.clone(),
    ));

    let sessions_service = Arc::new(SessionsService::new(
        sessions_repo.clone(),
        clock_trait.clone(),
        replay_engine.clone(),
    ));

    let state = AppState {
        config: config.clone(),
        market_service,
        ingest_service,
        sessions_service,
        orders_service,
        account_service,
        replay_service,
        broadcaster,
    };

    Ok(create_router(state))
}
