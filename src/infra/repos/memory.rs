// src/infra/repos/memory.rs
use std::collections::HashMap;

use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    domain::{
        models::{AccountSnapshot, Fill, Order, OrderStatus, SessionConfig, SessionStatus},
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

    async fn set_enabled(&self, session_id: Uuid, enabled: bool) -> Result<(), AppError> {
        let mut guard = self.inner.write().await;
        let entry = guard
            .get_mut(&session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))?;
        entry.enabled = enabled;
        entry.updated_at = TimestampMs::from(Utc::now().timestamp_millis());
        Ok(())
    }

    async fn delete(&self, session_id: Uuid) -> Result<(), AppError> {
        let mut guard = self.inner.write().await;
        guard
            .remove(&session_id)
            .map(|_| ())
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))
    }
}

#[derive(Clone)]
struct OrderEntry {
    order: Order,
    fills: Vec<Fill>,
}

#[derive(Default)]
pub struct MemoryOrdersRepo {
    inner: RwLock<HashMap<Uuid, HashMap<Uuid, OrderEntry>>>,
}

impl MemoryOrdersRepo {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    fn get_entry_mut<'a>(
        guard: &'a mut HashMap<Uuid, HashMap<Uuid, OrderEntry>>,
        session_id: Uuid,
        order_id: Uuid,
    ) -> Result<&'a mut OrderEntry, AppError> {
        guard
            .get_mut(&session_id)
            .and_then(|orders| orders.get_mut(&order_id))
            .ok_or_else(|| AppError::NotFound(format!("order {order_id} not found")))
    }
}

#[async_trait::async_trait]
impl OrdersRepo for MemoryOrdersRepo {
    async fn create(&self, order: Order) -> Result<Order, AppError> {
        let mut guard = self.inner.write().await;
        let session_orders = guard.entry(order.session_id).or_default();
        session_orders.insert(
            order.id,
            OrderEntry {
                order: order.clone(),
                fills: Vec::new(),
            },
        );
        Ok(order)
    }

    async fn update(&self, order: Order) -> Result<Order, AppError> {
        let mut guard = self.inner.write().await;
        let session_orders = guard
            .get_mut(&order.session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {} not found", order.session_id)))?;
        let entry = session_orders
            .get_mut(&order.id)
            .ok_or_else(|| AppError::NotFound(format!("order {} not found", order.id)))?;
        entry.order = order.clone();
        Ok(order)
    }

    async fn get(&self, session_id: Uuid, order_id: Uuid) -> Result<Order, AppError> {
        let guard = self.inner.read().await;
        guard
            .get(&session_id)
            .and_then(|orders| orders.get(&order_id))
            .map(|entry| entry.order.clone())
            .ok_or_else(|| AppError::NotFound(format!("order {order_id} not found")))
    }

    async fn get_by_client_id(&self, session_id: Uuid, client_id: &str) -> Result<Order, AppError> {
        let guard = self.inner.read().await;
        let session_orders = guard
            .get(&session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))?;
        session_orders
            .values()
            .find(|entry| entry.order.client_order_id.as_deref() == Some(client_id))
            .map(|entry| entry.order.clone())
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
            .filter(|entry| {
                matches!(
                    entry.order.status,
                    OrderStatus::New | OrderStatus::PartiallyFilled
                )
            })
            .map(|entry| entry.order.clone())
            .collect();
        if let Some(symbol) = symbol {
            out.retain(|order| order.symbol == symbol);
        }
        Ok(out)
    }

    async fn list_active(&self, session_id: Uuid) -> Result<Vec<Order>, AppError> {
        let guard = self.inner.read().await;
        let session_orders = guard
            .get(&session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))?;
        Ok(session_orders
            .values()
            .filter(|entry| {
                matches!(
                    entry.order.status,
                    OrderStatus::New | OrderStatus::PartiallyFilled
                )
            })
            .map(|entry| entry.order.clone())
            .collect())
    }

    async fn cancel(&self, session_id: Uuid, order_id: Uuid) -> Result<Order, AppError> {
        let mut guard = self.inner.write().await;
        let entry = Self::get_entry_mut(&mut guard, session_id, order_id)?;
        entry.order.status = OrderStatus::Canceled;
        entry.order.updated_at = TimestampMs::from(Utc::now().timestamp_millis());
        Ok(entry.order.clone())
    }

    async fn mark_expired_for_session(&self, session_id: Uuid) -> Result<Vec<Order>, AppError> {
        let mut guard = self.inner.write().await;
        let session_orders = guard
            .get_mut(&session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))?;
        let mut updated = Vec::new();
        for entry in session_orders.values_mut() {
            if matches!(
                entry.order.status,
                OrderStatus::New | OrderStatus::PartiallyFilled
            ) {
                entry.order.status = OrderStatus::Expired;
                entry.order.updated_at = TimestampMs::from(Utc::now().timestamp_millis());
                updated.push(entry.order.clone());
            }
        }
        Ok(updated)
    }

    async fn append_fill(&self, fill: Fill) -> Result<(), AppError> {
        let mut guard = self.inner.write().await;
        let session_orders = guard
            .get_mut(&fill.session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {} not found", fill.session_id)))?;
        let entry = session_orders
            .get_mut(&fill.order_id)
            .ok_or_else(|| AppError::NotFound(format!("order {} not found", fill.order_id)))?;
        if entry
            .fills
            .iter()
            .any(|existing| existing.trade_id == fill.trade_id)
        {
            return Ok(());
        }
        entry.fills.push(fill);
        Ok(())
    }

    async fn list_fills(
        &self,
        session_id: Uuid,
        symbol: Option<&str>,
    ) -> Result<Vec<Fill>, AppError> {
        let guard = self.inner.read().await;
        let session_orders = guard
            .get(&session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))?;
        let mut fills = Vec::new();
        for entry in session_orders.values() {
            if let Some(symbol) = symbol {
                if entry.order.symbol != symbol {
                    continue;
                }
            }
            fills.extend(entry.fills.iter().cloned());
        }
        fills.sort_by_key(|fill| fill.event_time.0);
        Ok(fills)
    }

    async fn list_order_fills(
        &self,
        session_id: Uuid,
        order_id: Uuid,
    ) -> Result<Vec<Fill>, AppError> {
        let guard = self.inner.read().await;
        let session_orders = guard
            .get(&session_id)
            .ok_or_else(|| AppError::NotFound(format!("session {session_id} not found")))?;
        let entry = session_orders
            .get(&order_id)
            .ok_or_else(|| AppError::NotFound(format!("order {order_id} not found")))?;
        Ok(entry.fills.clone())
    }

    async fn has_fill(&self, order_id: Uuid, trade_id: i64) -> Result<bool, AppError> {
        let guard = self.inner.read().await;
        for session_orders in guard.values() {
            if let Some(entry) = session_orders.get(&order_id) {
                return Ok(entry
                    .fills
                    .iter()
                    .any(|existing| existing.trade_id == trade_id));
            }
        }
        Ok(false)
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
