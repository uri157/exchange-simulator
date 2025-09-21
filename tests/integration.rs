use std::sync::Arc;
use std::time::Duration;

use tempfile::tempdir;
use tokio::time::timeout;

use duckdb::params;
use exchange_simulator::domain::{
    models::{MarketMode, OrderSide, OrderStatus, OrderType},
    traits::{AccountsRepo, MarketStore, OrdersRepo, ReplayEngine, SessionsRepo},
    value_objects::{Interval, Quantity, Speed, TimestampMs},
};
use exchange_simulator::error::AppError;
use exchange_simulator::infra::{
    clock::SimulatedClock,
    duckdb::{db::DuckDbPool, market_repo::DuckDbMarketStore},
    repos::memory::{MemoryAccountsRepo, MemoryOrdersRepo, MemorySessionsRepo},
    ws::broadcaster::SessionBroadcaster,
};
use exchange_simulator::services::{
    account_service::AccountService, market_service::MarketService, orders_service::OrdersService,
    replay_service::ReplayService, sessions_service::SessionsService,
};

#[tokio::test]
async fn ingest_replay_and_order_flow() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("market.duckdb");
    let pool = DuckDbPool::new(db_path.to_str().unwrap()).unwrap();

    let market_store: Arc<dyn MarketStore> = Arc::new(DuckDbMarketStore::new(pool.clone()));
    const INTERVAL_MS: i64 = 60_000;
    const NUM_KLINES: i64 = 10;

    pool.with_conn(|conn| {
        conn.execute(
            "INSERT INTO symbols (symbol, base, quote, active) VALUES (?1, ?2, ?3, TRUE)",
            params!["BTCUSDT", "BTC", "USDT"],
        )
        .map_err(|err| AppError::Database(format!("insert symbol failed: {err}")))?;
        for i in 0..NUM_KLINES {
            let open_time = i * INTERVAL_MS;
            let close_time = open_time + INTERVAL_MS;
            let base_price = 100.0 + i as f64;
            conn.execute(
                "INSERT INTO klines (symbol, interval, open_time, open, high, low, close, volume, close_time) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    "BTCUSDT",
                    "1m",
                    open_time,
                    base_price,
                    base_price + 10.0,
                    base_price - 10.0,
                    base_price + 5.0,
                    10.0 + i as f64,
                    close_time
                ],
            )
            .map_err(|err| AppError::Database(format!("insert kline failed: {err}")))?;
        }
        Ok(())
    })
    .unwrap();

    let market_service = Arc::new(MarketService::new(market_store.clone()));
    assert_eq!(market_service.exchange_info().await.unwrap().len(), 1);

    let sessions_repo: Arc<dyn SessionsRepo> = Arc::new(MemorySessionsRepo::new());
    let orders_repo: Arc<dyn OrdersRepo> = Arc::new(MemoryOrdersRepo::new());
    let accounts_repo: Arc<dyn AccountsRepo> = Arc::new(MemoryAccountsRepo::new());
    let clock = Arc::new(SimulatedClock::new(Speed(1.0)));
    let clock_trait: Arc<dyn exchange_simulator::domain::traits::Clock> = clock.clone();
    let broadcaster = SessionBroadcaster::new(16);

    let replay_service = Arc::new(ReplayService::new(
        market_store.clone(),
        clock_trait.clone(),
        sessions_repo.clone(),
        broadcaster.clone(),
    ));
    let replay_engine: Arc<dyn ReplayEngine> = replay_service.clone();

    let account_service = Arc::new(AccountService::new(
        accounts_repo.clone(),
        "USDT".to_string(),
        1_000.0,
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
        broadcaster.clone(),
        MarketMode::Kline,
    ));

    let session = sessions_service
        .create_session(
            vec!["BTCUSDT".to_string()],
            Interval::new("1m"),
            TimestampMs(0),
            TimestampMs(NUM_KLINES * INTERVAL_MS),
            Speed(1.0),
            42,
            None,
        )
        .await
        .unwrap();
    let mut receiver = broadcaster.subscribe(session.session_id).await.unwrap();
    sessions_service
        .start_session(session.session_id)
        .await
        .unwrap();
    let message = timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("stream message")
        .unwrap();
    assert!(message.contains("kline"));

    sessions_service
        .pause_session(session.session_id)
        .await
        .unwrap();

    let (order, fills) = orders_service
        .place_order(
            session.session_id,
            "BTCUSDT".to_string(),
            OrderSide::Buy,
            OrderType::Market,
            Quantity(1.0),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(fills.len(), 1);

    let account = account_service
        .get_account(session.session_id)
        .await
        .unwrap();
    assert!(account
        .balances
        .iter()
        .any(|balance| balance.asset == "BTC" && balance.free.0 > 0.0));
}
