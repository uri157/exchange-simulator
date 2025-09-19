// src/infra/ws/broadcaster.rs
use std::{collections::HashMap, sync::Arc};

use serde::Serialize;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::error::AppError;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsKlineData {
    pub event: &'static str,
    pub symbol: String,
    pub interval: String,
    pub open_time: i64,
    pub close_time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct WsBroadcastMessage {
    pub stream: String,
    pub data: WsKlineData,
}

impl WsBroadcastMessage {
    pub fn to_json_string(&self) -> Result<String, AppError> {
        serde_json::to_string(self)
            .map_err(|err| AppError::Internal(format!("serialize ws message failed: {err}")))
    }

    pub fn stream(&self) -> &str {
        &self.stream
    }

    pub fn close_time(&self) -> i64 {
        self.data.close_time
    }

    pub fn example_json() -> String {
        let example = Self {
            stream: "kline@1m:BTCUSDT".to_string(),
            data: WsKlineData {
                event: "kline",
                symbol: "BTCUSDT".to_string(),
                interval: "1m".to_string(),
                open_time: 0,
                close_time: 60_000,
                open: 0.0,
                high: 0.0,
                low: 0.0,
                close: 0.0,
                volume: 0.0,
            },
        };

        example
            .to_json_string()
            .unwrap_or_else(|_| "{}".to_string())
    }
}

#[derive(Clone)]
pub struct SessionBroadcaster {
    inner: Arc<RwLock<HashMap<Uuid, broadcast::Sender<WsBroadcastMessage>>>>,
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
    ) -> Result<broadcast::Receiver<WsBroadcastMessage>, AppError> {
        let mut guard = self.inner.write().await;
        let sender = guard.entry(session_id).or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(self.buffer);
            tx
        });
        Ok(sender.subscribe())
    }

    pub async fn get_sender(&self, session_id: Uuid) -> broadcast::Sender<WsBroadcastMessage> {
        let mut guard = self.inner.write().await;
        guard
            .entry(session_id)
            .or_insert_with(|| broadcast::channel(self.buffer).0)
            .clone()
    }

    pub async fn broadcast(
        &self,
        session_id: Uuid,
        message: WsBroadcastMessage,
    ) -> Result<(), AppError> {
        let sender = self.get_sender(session_id).await;
        sender
            .send(message)
            .map(|_| ())
            .map_err(|err| AppError::Internal(format!("ws broadcast failed: {err}")))
    }

    pub async fn close_session(&self, session_id: Uuid) {
        let mut guard = self.inner.write().await;
        guard.remove(&session_id);
    }
}
