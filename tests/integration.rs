use std::sync::Arc;
use std::time::Duration;

use tempfile::tempdir;
use tokio::time::timeout;

use exchange_simulator::domain::{
    models::{DatasetFormat, OrderSide, OrderStatus, OrderType},
    traits::{AccountsRepo, MarketIngestor, MarketStore, OrdersRepo, ReplayEngine, SessionsRepo},
    value_objects::{DatasetPath, Interval, Quantity, Speed, TimestampMs},
};
use exchange_simulator::infra::{
    clock::SimulatedClock,
    duckdb::{db::DuckDbPool, ingest_repo::DuckDbIngestRepo, market_repo::DuckDbMarketStore},
    repos::memory::{MemoryAccountsRepo, MemoryOrdersRepo, MemorySessionsRepo},
    ws::broadcaster::SessionBroadcaster,
};
use exchange_simulator::services::{
    account_service::AccountService, ingest_service::IngestService, market_service::MarketService,
    orders_service::OrdersService, replay_service::ReplayService,
    sessions_service::SessionsService,
};

#[tokio::test]
async fn ingest_replay_and_order_flow() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("market.duckdb");
    let pool = DuckDbPool::new(db_path.to_str().unwrap()).unwrap();

    let csv_path = tmp.path().join("klines.csv");
    std::fs::write(
        &csv_path,
        "symbol,interval,open_time,open,high,low,close,volume,close_time\nBTCUSDT,1m,0,100,110,90,105,10,60000\nBTCUSDT,1m,60000,105,115,100,110,12,120000\n",
    )
    .unwrap();

    let market_store: Arc<dyn MarketStore> = Arc::new(DuckDbMarketStore::new(pool.clone()));
    let ingestor: Arc<dyn MarketIngestor> = Arc::new(DuckDbIngestRepo::new(pool.clone()));
    let ingest_service = Arc::new(IngestService::new(ingestor.clone()));
    let dataset = ingest_service
        .register_dataset(
            "test",
            DatasetPath::from(csv_path.to_string_lossy().to_string()),
            DatasetFormat::Csv,
        )
        .await
        .unwrap();
    ingest_service.ingest_dataset(dataset.id).await.unwrap();

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
    ));

    let session = sessions_service
        .create_session(
            vec!["BTCUSDT".to_string()],
            Interval::new("1m"),
            TimestampMs(0),
            TimestampMs(120000),
            Speed(1.0),
            42,
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
