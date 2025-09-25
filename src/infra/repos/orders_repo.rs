use std::collections::HashMap;

use tokio::sync::RwLock;
use uuid::Uuid;

/// Simple in-memory mapping between UUID order identifiers and the
/// Binance-compatible incremental `orderId` exposed through the API.
#[derive(Default)]
pub struct OrderIdMapping {
    inner: RwLock<OrderIdMappingInner>,
}

#[derive(Default)]
struct OrderIdMappingInner {
    /// Monotonic counters per session. The Binance API exposes numerical
    /// identifiers, so we keep a per-session counter to make ids compact
    /// while still unique within the session scope.
    counters: HashMap<Uuid, u64>,
    /// Map from (session, order uuid) -> numeric id.
    by_uuid: HashMap<(Uuid, Uuid), u64>,
    /// Map from (session, numeric id) -> order uuid.
    by_numeric: HashMap<(Uuid, u64), Uuid>,
}

impl OrderIdMapping {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the numeric id assigned to the order, creating one if needed.
    pub async fn ensure_mapping(&self, session_id: Uuid, order_id: Uuid) -> u64 {
        let mut guard = self.inner.write().await;
        if let Some(existing) = guard.by_uuid.get(&(session_id, order_id)) {
            return *existing;
        }

        let counter = guard.counters.entry(session_id).or_insert(0);
        *counter += 1;
        let numeric = *counter;
        guard.by_uuid.insert((session_id, order_id), numeric);
        guard.by_numeric.insert((session_id, numeric), order_id);
        numeric
    }

    /// Retrieves the numeric id for an existing order, if present.
    pub async fn get_numeric(&self, session_id: Uuid, order_id: Uuid) -> Option<u64> {
        let guard = self.inner.read().await;
        guard.by_uuid.get(&(session_id, order_id)).copied()
    }

    /// Resolves an exposed numeric id back to the internal order uuid.
    pub async fn resolve_uuid(&self, session_id: Uuid, order_id: u64) -> Option<Uuid> {
        let guard = self.inner.read().await;
        guard.by_numeric.get(&(session_id, order_id)).copied()
    }
}
