use std::collections::HashMap;

use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    domain::{
        models::{AccountSnapshot, Order, OrderStatus, SessionConfig, SessionStatus},
        traits::{AccountsRepo, OrdersRepo, SessionsRepo},
        value_objects::{Speed, TimestampMs},
    },
    error::AppError,
};

#[derive(Default)]
pub struct MemorySessionsRepo {
    inner: RwLock<HashMap<Uuid, SessionConfig>>,
}

impl MemorySessionsRepo {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl SessionsRepo for MemorySessionsRepo {
    async fn insert(&self, config: SessionConfig) -> Result<SessionConfig, AppError> {
        let mut guard = self.inner.write().await;
        guard.insert(config.session_id, config.clone());
        Ok(config)
    }

    async fn update_status(
        &self,
        session_id: Uuid,
        status: SessionStatus,
    ) -> Result<SessionConfig, AppError> {
        let mut guard = self.inner.write().await;
        let entry = guard
            .get_mut(&session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))?;
        entry.status = status;
        entry.updated_at = TimestampMs::from(Utc::now().timestamp_millis());
        Ok(entry.clone())
    }

    async fn update_speed(
        &self,
        session_id: Uuid,
        speed: Speed,
    ) -> Result<SessionConfig, AppError> {
        let mut guard = self.inner.write().await;
        let entry = guard
            .get_mut(&session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))?;
        entry.speed = speed;
        entry.updated_at = TimestampMs::from(Utc::now().timestamp_millis());
        Ok(entry.clone())
    }

    async fn get(&self, session_id: Uuid) -> Result<SessionConfig, AppError> {
        let guard = self.inner.read().await;
        guard
            .get(&session_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))
    }

    async fn list(&self) -> Result<Vec<SessionConfig>, AppError> {
        let guard = self.inner.read().await;
        Ok(guard.values().cloned().collect())
    }
}

#[derive(Default)]
pub struct MemoryOrdersRepo {
    inner: RwLock<HashMap<Uuid, HashMap<Uuid, Order>>>,
}

impl MemoryOrdersRepo {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl OrdersRepo for MemoryOrdersRepo {
    async fn upsert(&self, order: Order) -> Result<Order, AppError> {
        let mut guard = self.inner.write().await;
        let session_orders = guard.entry(order.session_id).or_default();
        session_orders.insert(order.order_id, order.clone());
        Ok(order)
    }

    async fn get(&self, session_id: Uuid, order_id: Uuid) -> Result<Order, AppError> {
        let guard = self.inner.read().await;
        guard
            .get(&session_id)
            .and_then(|orders| orders.get(&order_id).cloned())
            .ok_or_else(|| AppError::NotFound(format!("order {order_id} not found")))
    }

    async fn get_by_client_id(&self, session_id: Uuid, client_id: &str) -> Result<Order, AppError> {
        let guard = self.inner.read().await;
        let session_orders = guard
            .get(&session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))?;
        session_orders
            .values()
            .find(|order| order.client_order_id.as_deref() == Some(client_id))
            .cloned()
            .ok_or_else(|| {
                AppError::NotFound(format!("order with client id {client_id} not found"))
            })
    }

    async fn list_open(
        &self,
        session_id: Uuid,
        symbol: Option<&str>,
    ) -> Result<Vec<Order>, AppError> {
        let guard = self.inner.read().await;
        let session_orders = guard
            .get(&session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))?;
        let mut out: Vec<Order> = session_orders
            .values()
            .filter(|order| {
                matches!(
                    order.status,
                    OrderStatus::New | OrderStatus::PartiallyFilled
                )
            })
            .cloned()
            .collect();
        if let Some(symbol) = symbol {
            out.retain(|order| order.symbol == symbol);
        }
        Ok(out)
    }
}

#[derive(Default)]
pub struct MemoryAccountsRepo {
    inner: RwLock<HashMap<Uuid, AccountSnapshot>>,
}

impl MemoryAccountsRepo {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl AccountsRepo for MemoryAccountsRepo {
    async fn get_account(&self, session_id: Uuid) -> Result<AccountSnapshot, AppError> {
        let guard = self.inner.read().await;
        guard.get(&session_id).cloned().ok_or_else(|| {
            AppError::NotFound(format!("account for session {session_id} not found"))
        })
    }

    async fn save_account(&self, snapshot: AccountSnapshot) -> Result<(), AppError> {
        let mut guard = self.inner.write().await;
        guard.insert(snapshot.session_id, snapshot);
        Ok(())
    }
}
