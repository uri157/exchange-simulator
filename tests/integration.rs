use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use tempfile::tempdir;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

use exchange_simulator::domain::{
    models::{DatasetMetadata, OrderSide, OrderStatus, OrderType},
    traits::{AccountsRepo, MarketIngestor, MarketStore, OrdersRepo, ReplayEngine, SessionsRepo},
    value_objects::{Interval, Quantity, Speed, TimestampMs},
};
use exchange_simulator::error::AppError;
use exchange_simulator::infra::{
    clock::SimulatedClock,
    duckdb::{db::DuckDbPool, ingest_sql, market_repo::DuckDbMarketStore},
    repos::memory::{MemoryAccountsRepo, MemoryOrdersRepo, MemorySessionsRepo},
    ws::broadcaster::SessionBroadcaster,
};
use exchange_simulator::services::{
    account_service::AccountService, ingest_service::IngestService, market_service::MarketService,
    orders_service::OrdersService, replay_service::ReplayService,
    sessions_service::SessionsService,
};

struct TestIngestRepo {
    pool: DuckDbPool,
    csv_path: PathBuf,
    datasets: Mutex<HashMap<Uuid, DatasetMetadata>>,
}

impl TestIngestRepo {
    fn new(pool: DuckDbPool, csv_path: PathBuf) -> Self {
        Self {
            pool,
            csv_path,
            datasets: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl MarketIngestor for TestIngestRepo {
    async fn register_dataset(
        &self,
        symbol: &str,
        interval: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<DatasetMetadata, AppError> {
        if start_time <= 0 && end_time <= 0 {
            return Err(AppError::Validation(
                "invalid time range for dataset".to_string(),
            ));
        }
        if end_time <= start_time {
            return Err(AppError::Validation(
                "invalid time range for dataset".to_string(),
            ));
        }

        let meta = DatasetMetadata {
            id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            interval: interval.to_string(),
            start_time,
            end_time,
            status: "registered".to_string(),
            created_at: Utc::now().timestamp_millis(),
        };

        {
            let mut guard = self.datasets.lock().unwrap();
            guard.insert(meta.id, meta.clone());
        }

        let pool = self.pool.clone();
        let meta_for_insert = meta.clone();
        pool.with_conn_async(move |conn| {
            ingest_sql::insert_dataset_row(conn, &meta_for_insert)?;
            Ok::<_, AppError>(())
        })
        .await?;

        Ok(meta)
    }

    async fn list_datasets(&self) -> Result<Vec<DatasetMetadata>, AppError> {
        let guard = self.datasets.lock().unwrap();
        Ok(guard.values().cloned().collect())
    }

    async fn ingest_dataset(&self, dataset_id: Uuid) -> Result<(), AppError> {
        let meta = {
            let guard = self.datasets.lock().unwrap();
            guard
                .get(&dataset_id)
                .cloned()
                .ok_or_else(|| AppError::NotFound(format!("dataset {dataset_id} not found")))?
        };

        let content = std::fs::read_to_string(&self.csv_path)
            .map_err(|e| AppError::Internal(format!("failed to read csv: {e}")))?;

        let mut rows: Vec<Vec<Value>> = Vec::new();
        for line in content.lines().skip(1).filter(|l| !l.trim().is_empty()) {
            let cols: Vec<&str> = line.split(',').collect();
            if cols.len() < 9 {
                return Err(AppError::Validation("invalid csv row".to_string()));
            }
            let open_time: i64 = cols[2]
                .parse()
                .map_err(|e| AppError::Validation(format!("open_time parse error: {e}")))?;
            let close_time: i64 = cols[8]
                .parse()
                .map_err(|e| AppError::Validation(format!("close_time parse error: {e}")))?;
            rows.push(vec![
                Value::from(open_time),
                Value::String(cols[3].to_string()),
                Value::String(cols[4].to_string()),
                Value::String(cols[5].to_string()),
                Value::String(cols[6].to_string()),
                Value::String(cols[7].to_string()),
                Value::from(close_time),
            ]);
        }

        let pool = self.pool.clone();
        let symbol = meta.symbol.clone();
        let interval = meta.interval.clone();
        pool.with_conn_async(move |conn| {
            ingest_sql::insert_symbols_if_needed(conn, &symbol)?;
            ingest_sql::insert_klines_chunk(conn, &symbol, &interval, &rows)?;
            ingest_sql::mark_dataset_status(conn, dataset_id, "ready")?;
            Ok::<_, AppError>(())
        })
        .await?;

        let mut guard = self.datasets.lock().unwrap();
        if let Some(entry) = guard.get_mut(&dataset_id) {
            entry.status = "ready".to_string();
        }

        Ok(())
    }

    async fn list_ready_symbols(&self) -> Result<Vec<String>, AppError> {
        let guard = self.datasets.lock().unwrap();
        let symbols: HashSet<String> = guard
            .values()
            .filter(|meta| meta.status == "ready")
            .map(|meta| meta.symbol.clone())
            .collect();
        Ok(symbols.into_iter().collect())
    }

    async fn list_ready_intervals(&self, symbol: &str) -> Result<Vec<String>, AppError> {
        let guard = self.datasets.lock().unwrap();
        let intervals: HashSet<String> = guard
            .values()
            .filter(|meta| meta.status == "ready" && meta.symbol == symbol)
            .map(|meta| meta.interval.clone())
            .collect();
        Ok(intervals.into_iter().collect())
    }

    async fn get_range(&self, symbol: &str, interval: &str) -> Result<(i64, i64), AppError> {
        let pool = self.pool.clone();
        let sym = symbol.to_string();
        let intv = interval.to_string();
        let maybe_range = pool
            .with_conn_async(move |conn| {
                ingest_sql::get_range_for_symbol_interval(conn, &sym, &intv)
            })
            .await?;

        maybe_range.ok_or_else(|| {
            AppError::NotFound(format!(
                "range for symbol {symbol} interval {interval} not found"
            ))
        })
    }
}

#[tokio::test]
async fn ingest_replay_and_order_flow() {
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("market.duckdb");
    let pool = DuckDbPool::new(db_path.to_str().unwrap()).unwrap();

    let csv_path = tmp.path().join("klines.csv");
    std::fs::write(
        &csv_path,
        [
            "symbol,interval,open_time,open,high,low,close,volume,close_time",
            "BTCUSDT,1m,0,100,110,90,105,10,60000",
            "BTCUSDT,1m,120000,105,115,100,110,12,180000",
            "BTCUSDT,1m,240000,110,120,105,115,15,300000",
            "BTCUSDT,1m,360000,115,125,110,120,20,420000",
            "BTCUSDT,1m,480000,120,130,115,125,18,540000",
            "BTCUSDT,1m,600000,125,135,120,130,16,660000",
        ]
        .join("\n"),
    )
    .unwrap();

    let market_store: Arc<dyn MarketStore> = Arc::new(DuckDbMarketStore::new(pool.clone()));
    let ingestor: Arc<dyn MarketIngestor> =
        Arc::new(TestIngestRepo::new(pool.clone(), csv_path.clone()));
    let ingest_service = Arc::new(IngestService::new(ingestor.clone()));
    let dataset = ingest_service
        .register_dataset("BTCUSDT", "1m", 0, 660000)
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
            TimestampMs(660000),
            Speed(0.1),
            42,
        )
        .await
        .unwrap();
    let mut receiver = broadcaster.subscribe(session.session_id).await.unwrap();
    sessions_service
        .start_session(session.session_id)
        .await
        .unwrap();
    let order_handle = {
        let orders_service = orders_service.clone();
        let session_id = session.session_id;
        tokio::spawn(async move {
            loop {
                match orders_service
                    .place_order(
                        session_id,
                        "BTCUSDT".to_string(),
                        OrderSide::Buy,
                        OrderType::Market,
                        Quantity(1.0),
                        None,
                        None,
                    )
                    .await
                {
                    Ok(result) => break result,
                    Err(AppError::Validation(msg))
                        if msg.contains("no market data for session yet") =>
                    {
                        sleep(Duration::from_millis(10)).await;
                    }
                    Err(err) => panic!("order placement failed: {err:?}"),
                }
            }
        })
    };
    let message = timeout(Duration::from_secs(5), receiver.recv())
        .await
        .expect("stream message")
        .unwrap();
    assert!(message.contains("kline"));

    let (order, fills) = order_handle.await.unwrap();
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
