use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct OrderIdMapping {
    // contador por sesión
    counters: Arc<RwLock<HashMap<Uuid, u64>>>,
    // (session, long) -> uuid
    by_long: Arc<RwLock<HashMap<(Uuid, u64), Uuid>>>,
    // (session, uuid) -> long
    by_uuid: Arc<RwLock<HashMap<(Uuid, Uuid), u64>>>,
}

impl OrderIdMapping {
    pub fn new() -> Self {
        Self::default()
    }

    /// Devuelve el long id existente para (session, uuid) o crea uno nuevo incremental por sesión.
    pub async fn ensure_mapping(&self, session: Uuid, order_uuid: Uuid) -> u64 {
        // 1) fast path: ya existe
        if let Some(existing) = self
            .by_uuid
            .read()
            .await
            .get(&(session, order_uuid))
            .copied()
        {
            return existing;
        }
        // 2) crear nuevo id incremental por sesión
        let next = {
            let mut counters = self.counters.write().await;
            let ctr = counters.entry(session).or_insert(0);
            *ctr += 1;
            *ctr
        };
        {
            let mut by_long = self.by_long.write().await;
            by_long.insert((session, next), order_uuid);
        }
        {
            let mut by_uuid = self.by_uuid.write().await;
            by_uuid.insert((session, order_uuid), next);
        }
        next
    }

    /// (session, long) -> uuid
    pub async fn resolve_uuid(&self, session: Uuid, order_id_long: u64) -> Option<Uuid> {
        self.by_long
            .read()
            .await
            .get(&(session, order_id_long))
            .copied()
    }

    /// (session, uuid) -> long
    pub async fn resolve_long(&self, session: Uuid, order_uuid: Uuid) -> Option<u64> {
        self.by_uuid
            .read()
            .await
            .get(&(session, order_uuid))
            .copied()
    }

    /// Útil para tests o tooling
    pub async fn clear_session(&self, session: Uuid) {
        // limpiar tablas de esa sesión
        {
            let mut by_long = self.by_long.write().await;
            by_long.retain(|(s, _), _| *s != session);
        }
        {
            let mut by_uuid = self.by_uuid.write().await;
            by_uuid.retain(|(s, _), _| *s != session);
        }
        {
            let mut counters = self.counters.write().await;
            counters.remove(&session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mapping_is_per_session_and_idempotent() {
        let m = OrderIdMapping::new();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        let a1 = m.ensure_mapping(s1, a).await;
        let a1b = m.ensure_mapping(s1, a).await; // idempotente
        assert_eq!(a1, a1b);
        assert_eq!(m.resolve_uuid(s1, a1).await, Some(a));
        assert_eq!(m.resolve_long(s1, a).await, Some(a1));

        let b1 = m.ensure_mapping(s1, b).await;
        assert!(b1 > a1);

        let a_other = m.ensure_mapping(s2, a).await; // otra sesión reinicia
        assert_eq!(a_other, 1);
    }
}
