use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    domain::{
        models::{
            AccountSnapshot, AggTrade, DatasetMetadata, FeeConfig, Fill, Kline, Order,
            SessionConfig, SessionStatus, Symbol,
        },
        value_objects::{Interval, Speed, TimestampMs},
    },
    error::AppError,
};

#[async_trait]
pub trait MarketStore: Send + Sync {
    async fn list_symbols(&self) -> Result<Vec<Symbol>, AppError>;
    async fn get_klines(
        &self,
        symbol: &str,
        interval: &Interval,
        start: Option<TimestampMs>,
        end: Option<TimestampMs>,
        limit: Option<usize>,
    ) -> Result<Vec<Kline>, AppError>;
}

#[async_trait]
pub trait AggTradesStore: Send + Sync {
    async fn get_trades(
        &self,
        symbol: &str,
        from: Option<TimestampMs>,
        to: Option<TimestampMs>,
        limit: Option<usize>,
    ) -> Result<Vec<AggTrade>, AppError>;
}

#[async_trait]
pub trait MarketIngestor: Send + Sync {
    async fn register_dataset(
        &self,
        symbol: &str,
        interval: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<DatasetMetadata, AppError>;
    async fn list_datasets(&self) -> Result<Vec<DatasetMetadata>, AppError>;
    async fn ingest_dataset(&self, dataset_id: Uuid) -> Result<(), AppError>;
    async fn list_ready_symbols(&self) -> Result<Vec<String>, AppError>;
    async fn list_ready_intervals(&self, symbol: &str) -> Result<Vec<String>, AppError>;
    async fn get_range(&self, symbol: &str, interval: &str) -> Result<(i64, i64), AppError>;
}

#[async_trait]
pub trait SessionsRepo: Send + Sync {
    async fn insert(&self, config: SessionConfig) -> Result<SessionConfig, AppError>;
    async fn update_status(
        &self,
        session_id: Uuid,
        status: SessionStatus,
    ) -> Result<SessionConfig, AppError>;
    async fn update_speed(&self, session_id: Uuid, speed: Speed)
        -> Result<SessionConfig, AppError>;
    async fn get(&self, session_id: Uuid) -> Result<SessionConfig, AppError>;
    async fn list(&self) -> Result<Vec<SessionConfig>, AppError>;
    async fn set_enabled(&self, session_id: Uuid, enabled: bool) -> Result<(), AppError>;
    async fn delete(&self, session_id: Uuid) -> Result<(), AppError>;
}

#[async_trait]
pub trait OrdersRepo: Send + Sync {
    async fn create(&self, order: Order) -> Result<Order, AppError>;
    async fn update(&self, order: Order) -> Result<Order, AppError>;
    async fn get(&self, session_id: Uuid, order_id: Uuid) -> Result<Order, AppError>;
    async fn get_by_client_id(&self, session_id: Uuid, client_id: &str) -> Result<Order, AppError>;
    async fn list_open(
        &self,
        session_id: Uuid,
        symbol: Option<&str>,
    ) -> Result<Vec<Order>, AppError>;
    async fn list_active(&self, session_id: Uuid) -> Result<Vec<Order>, AppError>;
    async fn cancel(&self, session_id: Uuid, order_id: Uuid) -> Result<Order, AppError>;
    async fn mark_expired_for_session(&self, session_id: Uuid) -> Result<Vec<Order>, AppError>;
    async fn append_fill(&self, fill: Fill) -> Result<(), AppError>;
    async fn list_fills(
        &self,
        session_id: Uuid,
        symbol: Option<&str>,
    ) -> Result<Vec<Fill>, AppError>;
    async fn list_order_fills(
        &self,
        session_id: Uuid,
        order_id: Uuid,
    ) -> Result<Vec<Fill>, AppError>;
    async fn has_fill(&self, order_id: Uuid, trade_id: i64) -> Result<bool, AppError>;
}

#[async_trait]
pub trait AccountsRepo: Send + Sync {
    async fn get_account(&self, session_id: Uuid) -> Result<AccountSnapshot, AppError>;
    async fn save_account(&self, snapshot: AccountSnapshot) -> Result<(), AppError>;
}

#[async_trait]
pub trait Clock: Send + Sync {
    async fn init_session(&self, session_id: Uuid, start_time: TimestampMs)
        -> Result<(), AppError>;
    async fn now(&self, session_id: Uuid) -> Result<TimestampMs, AppError>;
    async fn set_speed(&self, session_id: Uuid, speed: Speed) -> Result<(), AppError>;
    async fn advance_to(&self, session_id: Uuid, to: TimestampMs) -> Result<(), AppError>;
    async fn pause(&self, session_id: Uuid) -> Result<(), AppError>;
    async fn resume(&self, session_id: Uuid) -> Result<(), AppError>;
    async fn is_paused(&self, session_id: Uuid) -> Result<bool, AppError>;
    async fn current_speed(&self, session_id: Uuid) -> Result<Speed, AppError>;
}

#[async_trait]
pub trait ReplayEngine: Send + Sync {
    async fn start(&self, session: SessionConfig) -> Result<(), AppError>;
    async fn pause(&self, session_id: Uuid) -> Result<(), AppError>;
    async fn resume(&self, session_id: Uuid) -> Result<(), AppError>;
    async fn seek(&self, session_id: Uuid, to: TimestampMs) -> Result<(), AppError>;
    async fn stop(&self, session_id: Uuid) -> Result<(), AppError>;
}

#[async_trait]
pub trait OrderBookSim: Send + Sync {}
