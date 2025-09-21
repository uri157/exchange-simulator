use std::{sync::Arc, time::Duration};

use axum::{
    body::Body,
    http::{Request, Response},
    Extension, Router,
};
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::{info, warn, Span};
use uuid::Uuid;

use crate::{
    app::router::create_router,
    domain::traits::{
        AccountsRepo, AggTradesStore, MarketIngestor, MarketStore, OrdersRepo, SessionsRepo,
    },
    infra::{
        clock::SimulatedClock,
        config::AppConfig,
        duckdb::{
            agg_trades_repo::DuckDbAggTradesStore, db::DuckDbPool, ingest_repo::DuckDbIngestRepo,
            market_repo::DuckDbMarketStore,
        },
        repos::{
            duckdb::DuckDbSessionsRepo,
            memory::{MemoryAccountsRepo, MemoryOrdersRepo},
        },
        ws::broadcaster::SessionBroadcaster,
    },
    services::{
        account_service::AccountService, market_service::MarketService,
        orders_service::OrdersService, replay_service::ReplayService,
        sessions_service::SessionsService, IngestService,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub market_service: Arc<MarketService>,
    pub ingest_service: Arc<IngestService>,
    pub agg_trades_store: Arc<dyn AggTradesStore>,
    pub sessions_service: Arc<SessionsService>,
    pub orders_service: Arc<OrdersService>,
    pub account_service: Arc<AccountService>,
    pub replay_service: Arc<ReplayService>,
    pub broadcaster: SessionBroadcaster,
    pub duck_pool: DuckDbPool,
}

/// Devuelve `Router<()>` con `Extension(AppState)` ya añadida.
/// Así `main` puede usar `.into_make_service()` sin problemas.
pub fn build_app(config: AppConfig) -> Result<Router, crate::error::AppError> {
    info!(duckdb_path = %config.duckdb_path, "opening DuckDB");
    let pool = DuckDbPool::new(&config.duckdb_path)?;

    // Warmup / métricas básicas (opcional)
    match pool.with_conn(|conn| {
        let count = |table: &str| -> Result<i64, crate::error::AppError> {
            let mut stmt = conn
                .prepare(&format!("SELECT COUNT(*) FROM {}", table))
                .map_err(|e| {
                    crate::error::AppError::Database(format!("prepare count {}: {e}", table))
                })?;
            let mut rows = stmt.query([]).map_err(|e| {
                crate::error::AppError::Database(format!("query count {}: {e}", table))
            })?;
            let row = rows
                .next()
                .map_err(|e| crate::error::AppError::Database(format!("row count {}: {e}", table)))?
                .ok_or_else(|| {
                    crate::error::AppError::Database(format!("no row for count {}", table))
                })?;
            let n: i64 = row.get(0).map_err(|e| {
                crate::error::AppError::Database(format!("get count {}: {e}", table))
            })?;
            Ok(n)
        };
        Ok::<_, crate::error::AppError>((count("datasets")?, count("klines")?, count("symbols")?))
    }) {
        Ok((ds, kl, sy)) => info!(datasets = ds, klines = kl, symbols = sy, "duckdb warmup"),
        Err(err) => warn!(error = %err, "duckdb warmup failed (continuing)"),
    }

    // Servicios
    let market_store_impl = Arc::new(DuckDbMarketStore::new(pool.clone()));
    let market_store: Arc<dyn MarketStore> = market_store_impl.clone();
    let agg_trades_store_impl = Arc::new(DuckDbAggTradesStore::new(pool.clone()));
    let agg_trades_store: Arc<dyn AggTradesStore> = agg_trades_store_impl.clone();
    let market_service = Arc::new(MarketService::new(market_store.clone()));

    let ingestor: Arc<dyn MarketIngestor> = Arc::new(DuckDbIngestRepo::new(pool.clone()));
    let ingest_service = Arc::new(IngestService::new(
        ingestor.clone(),
        market_store_impl.clone(),
        agg_trades_store_impl.clone(),
    ));

    let sessions_repo: Arc<dyn SessionsRepo> = Arc::new(DuckDbSessionsRepo::new(pool.clone())?);
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
        replay_engine,
        broadcaster.clone(),
        config.default_market_mode,
    ));

    let state = AppState {
        config: config.clone(),
        market_service,
        ingest_service,
        agg_trades_store,
        sessions_service,
        orders_service,
        account_service,
        replay_service,
        broadcaster,
        duck_pool: pool.clone(),
    };

    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|request: &Request<Body>| {
            let req_id = Uuid::new_v4();
            tracing::info_span!(
                "http_request",
                %req_id,
                method = %request.method(),
                path = %request.uri().path(),
                query = %request.uri().query().unwrap_or(""),
                status = tracing::field::Empty
            )
        })
        .on_request(|_request: &Request<Body>, span: &Span| {
            tracing::info!(parent: span, "request_started");
        })
        .on_response(|response: &Response<_>, latency: Duration, span: &Span| {
            span.record("status", &tracing::field::display(response.status()));
            tracing::info!(
                parent: span,
                status = %response.status(),
                latency_ms = latency.as_millis(),
                "request_finished"
            );
        })
        .on_failure(
            |failure: ServerErrorsFailureClass, latency: Duration, span: &Span| match failure {
                ServerErrorsFailureClass::StatusCode(status) => {
                    span.record("status", &tracing::field::display(status));
                    tracing::error!(
                        parent: span,
                        status = %status,
                        latency_ms = latency.as_millis(),
                        "request_failed"
                    );
                }
                ServerErrorsFailureClass::Error(err) => {
                    span.record("status", &tracing::field::display("error"));
                    tracing::error!(
                        parent: span,
                        error = %err,
                        latency_ms = latency.as_millis(),
                        "request_failed"
                    );
                }
            },
        );

    Ok(create_router().layer(trace_layer).layer(Extension(state)))
}
