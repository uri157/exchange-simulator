// src/infra/ws/broadcaster.rs
use std::{collections::HashMap, sync::Arc};

use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::error::AppError;

#[derive(Clone)]
pub struct SessionBroadcaster {
    inner: Arc<RwLock<HashMap<Uuid, broadcast::Sender<String>>>>,
    buffer: usize,
}

impl SessionBroadcaster {
    pub fn new(buffer: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            buffer,
        }
    }

    pub async fn subscribe(
        &self,
        session_id: Uuid,
    ) -> Result<broadcast::Receiver<String>, AppError> {
        let mut guard = self.inner.write().await;
        let sender = guard
            .entry(session_id)
            .or_insert_with(|| broadcast::channel(self.buffer).0);
        Ok(sender.subscribe())
    }

    pub async fn get_sender(&self, session_id: Uuid) -> broadcast::Sender<String> {
        let mut guard = self.inner.write().await;
        guard
            .entry(session_id)
            .or_insert_with(|| broadcast::channel(self.buffer).0)
            .clone()
    }

    pub async fn broadcast(&self, session_id: Uuid, message: String) -> Result<(), AppError> {
        let sender = self.get_sender(session_id).await;
        let _ = sender.send(message);
        Ok(())
    }

    /// Cierra el canal de una sesión (drop del sender) para que los clientes reciban `Closed`.
    pub async fn close(&self, session_id: Uuid) {
        let mut guard = self.inner.write().await;
        guard.remove(&session_id);
    }
}
