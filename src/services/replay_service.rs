use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{sync::RwLock, task::JoinHandle, time::sleep};
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    domain::{
        models::{Kline, SessionConfig, SessionStatus},
        traits::{Clock, MarketStore, ReplayEngine, SessionsRepo},
        value_objects::{Interval, TimestampMs},
    },
    error::AppError,
    infra::ws::broadcaster::SessionBroadcaster,
};

use super::ServiceResult;

pub struct ReplayService {
    market_store: Arc<dyn MarketStore>,
    clock: Arc<dyn Clock>,
    sessions_repo: Arc<dyn SessionsRepo>,
    broadcaster: SessionBroadcaster,
    tasks: Arc<RwLock<HashMap<Uuid, JoinHandle<()>>>>,
    latest: Arc<RwLock<HashMap<(Uuid, String), Kline>>>,
}

impl ReplayService {
    pub fn new(
        market_store: Arc<dyn MarketStore>,
        clock: Arc<dyn Clock>,
        sessions_repo: Arc<dyn SessionsRepo>,
        broadcaster: SessionBroadcaster,
    ) -> Self {
        Self {
            market_store,
            clock,
            sessions_repo,
            broadcaster,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            latest: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn latest_kline(
        &self,
        session_id: Uuid,
        symbol: &str,
    ) -> ServiceResult<Option<Kline>> {
        let guard = self.latest.read().await;
        Ok(guard.get(&(session_id, symbol.to_string())).cloned())
    }

    async fn cancel_task(&self, session_id: Uuid) {
        if let Some(handle) = self.tasks.write().await.remove(&session_id) {
            handle.abort();
        }
    }

    async fn run_session(self: Arc<Self>, mut session: SessionConfig, from: TimestampMs) {
        if session.start_time.0 > from.0 {
            session.start_time = from;
        }

        info!(session_id = %session.session_id, "starting replay session");

        if let Err(err) = self
            .sessions_repo
            .update_status(session.session_id, SessionStatus::Running)
            .await
        {
            error!(%err, "failed to set running status");
        }

        let mut timeline = Vec::new();
        for symbol in &session.symbols {
            match self
                .collect_klines(symbol, &session.interval, from, session.end_time)
                .await
            {
                Ok(data) => {
                    for kline in data {
                        timeline.push((symbol.clone(), kline));
                    }
                }
                Err(err) => {
                    error!(%err, "failed to load klines for symbol");
                    return;
                }
            }
        }

        timeline.sort_by_key(|(_, kline)| kline.open_time.0);

        let mut previous = from;
        for (symbol, kline) in timeline {
            if kline.open_time.0 > session.end_time.0 {
                break;
            }

            // Esperar si está pausado
            loop {
                match self.clock.is_paused(session.session_id).await {
                    Ok(paused) if paused => {
                        sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                    Ok(_) => break,
                    Err(err) => {
                        error!(%err, "clock lookup failed");
                        return;
                    }
                }
            }

            // Escalar el tiempo según la velocidad
            let speed = match self.clock.current_speed(session.session_id).await {
                Ok(speed) => speed,
                Err(err) => {
                    error!(%err, "speed lookup failed");
                    break;
                }
            };

            let delta = kline.open_time.0.saturating_sub(previous.0);
            if delta > 0 {
                let scaled = (delta as f64) / speed.0;
                let delay = Duration::from_millis(scaled.max(1.0) as u64);
                sleep(delay).await;
            }

            if let Err(err) = self
                .clock
                .advance_to(session.session_id, kline.close_time)
                .await
            {
                error!(%err, "clock advance failed");
            }

            {
                let mut guard = self.latest.write().await;
                guard.insert((session.session_id, symbol.clone()), kline.clone());
            }

            if let Err(err) = self
                .broadcaster
                .broadcast(
                    session.session_id,
                    serialize_kline(&symbol, &session.interval, &kline),
                )
                .await
            {
                error!(%err, "broadcast failed");
            }

            previous = kline.close_time;
        }

        if let Err(err) = self
            .sessions_repo
            .update_status(session.session_id, SessionStatus::Ended)
            .await
        {
            error!(%err, "failed to set session ended");
        }

        if let Err(err) = self.clock.pause(session.session_id).await {
            error!(%err, "failed to pause clock at end");
        }

        let _ = self.tasks.write().await.remove(&session.session_id);
    }

    async fn collect_klines(
        &self,
        symbol: &str,
        interval: &Interval,
        from: TimestampMs,
        end: TimestampMs,
    ) -> Result<Vec<Kline>, AppError> {
        let mut cursor = from;
        let mut out = Vec::new();

        loop {
            let batch = self
                .market_store
                .get_klines(symbol, interval, Some(cursor), Some(end), Some(1000))
                .await?;

            if batch.is_empty() {
                break;
            }

            let last_close = batch.last().map(|k| k.close_time).unwrap_or(cursor);
            out.extend(batch.into_iter());

            if last_close.0 >= end.0 {
                break;
            }
            cursor = TimestampMs(last_close.0 + 1);
        }

        Ok(out)
    }
}

fn serialize_kline(symbol: &str, interval: &Interval, kline: &Kline) -> String {
    let payload = json!({
        "stream": format!("{}@kline_{}", symbol.to_lowercase(), interval.as_str()),
        "data": {
            "e": "kline",
            "E": kline.close_time.0,
            "s": symbol,
            "k": {
                "t": kline.open_time.0,
                "T": kline.close_time.0,
                "s": symbol,
                "i": interval.as_str(),
                "o": format!("{:.6}", kline.open.0),
                "c": format!("{:.6}", kline.close.0),
                "h": format!("{:.6}", kline.high.0),
                "l": format!("{:.6}", kline.low.0),
                "v": format!("{:.6}", kline.volume.0),
                "x": true,
            }
        }
    });
    payload.to_string()
}

#[async_trait::async_trait]
impl ReplayEngine for ReplayService {
    async fn start(&self, session: SessionConfig) -> Result<(), AppError> {
        self.cancel_task(session.session_id).await;

        let service = Arc::new(self.clone_inner());
        let handle = tokio::spawn(
            service
                .clone()
                .run_session(session.clone(), session.start_time),
        );

        self.tasks.write().await.insert(session.session_id, handle);
        Ok(())
    }

    async fn pause(&self, session_id: Uuid) -> Result<(), AppError> {
        self.clock.pause(session_id).await
    }

    async fn resume(&self, session_id: Uuid) -> Result<(), AppError> {
        self.clock.resume(session_id).await
    }

    async fn seek(&self, session_id: Uuid, to: TimestampMs) -> Result<(), AppError> {
        self.cancel_task(session_id).await;

        let session = self.sessions_repo.get(session_id).await?;
        let service = Arc::new(self.clone_inner());
        let handle = tokio::spawn(service.clone().run_session(session.clone(), to));

        self.tasks.write().await.insert(session_id, handle);
        Ok(())
    }
}

impl ReplayService {
    fn clone_inner(&self) -> Self {
        Self {
            market_store: Arc::clone(&self.market_store),
            clock: Arc::clone(&self.clock),
            sessions_repo: Arc::clone(&self.sessions_repo),
            broadcaster: self.broadcaster.clone(),
            tasks: Arc::clone(&self.tasks),
            latest: Arc::clone(&self.latest),
        }
    }
}
