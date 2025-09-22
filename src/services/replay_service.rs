use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{sync::RwLock, task::JoinHandle, time::sleep};
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    domain::{
        models::{AggTrade, Kline, MarketMode, SessionConfig, SessionStatus},
        traits::{AggTradesStore, Clock, MarketStore, ReplayEngine, SessionsRepo},
        value_objects::{Interval, TimestampMs},
    },
    error::AppError,
    infra::ws::broadcaster::SessionBroadcaster,
};

use super::{matching_spot::SpotMatcher, ServiceResult};

pub struct ReplayService {
    market_store: Arc<dyn MarketStore>,
    agg_trades_store: Arc<dyn AggTradesStore>,
    clock: Arc<dyn Clock>,
    sessions_repo: Arc<dyn SessionsRepo>,
    broadcaster: SessionBroadcaster,
    spot_matcher: Option<Arc<SpotMatcher>>,
    tasks: Arc<RwLock<HashMap<Uuid, JoinHandle<()>>>>,
    latest_klines: Arc<RwLock<HashMap<(Uuid, String), Kline>>>,
    latest_trades: Arc<RwLock<HashMap<(Uuid, String), AggTrade>>>,
}

impl ReplayService {
    pub fn new(
        market_store: Arc<dyn MarketStore>,
        agg_trades_store: Arc<dyn AggTradesStore>,
        clock: Arc<dyn Clock>,
        sessions_repo: Arc<dyn SessionsRepo>,
        broadcaster: SessionBroadcaster,
        spot_matcher: Option<Arc<SpotMatcher>>,
    ) -> Self {
        Self {
            market_store,
            agg_trades_store,
            clock,
            sessions_repo,
            broadcaster,
            spot_matcher,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            latest_klines: Arc::new(RwLock::new(HashMap::new())),
            latest_trades: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn latest_kline(
        &self,
        session_id: Uuid,
        symbol: &str,
    ) -> ServiceResult<Option<Kline>> {
        let guard = self.latest_klines.read().await;
        Ok(guard.get(&(session_id, symbol.to_string())).cloned())
    }

    pub async fn latest_trade(
        &self,
        session_id: Uuid,
        symbol: &str,
    ) -> ServiceResult<Option<AggTrade>> {
        let guard = self.latest_trades.read().await;
        Ok(guard.get(&(session_id, symbol.to_string())).cloned())
    }

    async fn cancel_task(&self, session_id: Uuid) {
        if let Some(handle) = self.tasks.write().await.remove(&session_id) {
            handle.abort();
        }
    }

    async fn finalize_run(&self, session_id: Uuid) {
        let _ = self.tasks.write().await.remove(&session_id);
    }

    async fn run_session(self: Arc<Self>, session: SessionConfig, from: TimestampMs) {
        info!(session_id = %session.session_id, "starting replay session");

        if let Err(err) = self
            .sessions_repo
            .update_status(session.session_id, SessionStatus::Running)
            .await
        {
            error!(%err, "failed to set running status");
        }

        let result = match session.market_mode {
            MarketMode::Kline => self.run_kline_mode(&session, from).await,
            MarketMode::AggTrades => self.run_aggtrades_mode(&session, from).await,
        };

        if result.is_err() {
            self.finalize_run(session.session_id).await;
            return;
        }

        if let Err(err) = self
            .sessions_repo
            .update_status(session.session_id, SessionStatus::Ended)
            .await
        {
            error!(%err, "failed to set session ended");
        }

        if let Some(matcher) = &self.spot_matcher {
            matcher.on_session_end(session.session_id).await;
        }

        if let Err(err) = self.clock.pause(session.session_id).await {
            error!(%err, "failed to pause clock at end");
        }

        self.finalize_run(session.session_id).await;
    }

    async fn run_kline_mode(
        self: &Arc<Self>,
        session: &SessionConfig,
        from: TimestampMs,
    ) -> Result<(), ()> {
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
                    return Err(());
                }
            }
        }

        timeline.sort_by_key(|(_, kline)| kline.open_time.0);

        let mut previous = match self.clock.now(session.session_id).await {
            Ok(now) => now,
            Err(err) => {
                error!(%err, "failed to read clock time");
                from
            }
        };

        for (symbol, kline) in timeline {
            if kline.open_time.0 > session.end_time.0 {
                break;
            }

            loop {
                match self.clock.is_paused(session.session_id).await {
                    Ok(paused) if paused => {
                        sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                    Ok(_) => break,
                    Err(err) => {
                        error!(%err, "clock lookup failed");
                        return Err(());
                    }
                }
            }

            let speed = match self.clock.current_speed(session.session_id).await {
                Ok(speed) => speed,
                Err(err) => {
                    error!(%err, "speed lookup failed");
                    break;
                }
            };

            let current_clock = match self.clock.now(session.session_id).await {
                Ok(now) => now,
                Err(err) => {
                    error!(%err, "clock lookup failed");
                    break;
                }
            };

            if kline.close_time.0 <= current_clock.0 {
                previous = TimestampMs(previous.0.max(current_clock.0));
                continue;
            }

            let baseline = if current_clock.0 > previous.0 {
                current_clock
            } else {
                previous
            };

            let delta = kline.open_time.0.saturating_sub(baseline.0);
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
                previous = TimestampMs(previous.0.max(current_clock.0));
                continue;
            }

            {
                let mut guard = self.latest_klines.write().await;
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

        Ok(())
    }

    async fn run_aggtrades_mode(
        self: &Arc<Self>,
        session: &SessionConfig,
        from: TimestampMs,
    ) -> Result<(), ()> {
        let mut timeline: Vec<(String, AggTrade)> = Vec::new();
        for symbol in &session.symbols {
            match self.collect_trades(symbol, from, session.end_time).await {
                Ok(data) => {
                    for trade in data {
                        timeline.push((symbol.clone(), trade));
                    }
                }
                Err(err) => {
                    error!(%err, "failed to load agg trades for symbol");
                    return Err(());
                }
            }
        }

        timeline.sort_by(|(_, a), (_, b)| {
            a.event_time
                .0
                .cmp(&b.event_time.0)
                .then(a.trade_id.cmp(&b.trade_id))
        });

        let mut previous = match self.clock.now(session.session_id).await {
            Ok(now) => now,
            Err(err) => {
                error!(%err, "failed to read clock time");
                from
            }
        };

        for (symbol, trade) in timeline {
            if trade.event_time.0 > session.end_time.0 {
                break;
            }

            loop {
                match self.clock.is_paused(session.session_id).await {
                    Ok(paused) if paused => {
                        sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                    Ok(_) => break,
                    Err(err) => {
                        error!(%err, "clock lookup failed");
                        return Err(());
                    }
                }
            }

            let speed = match self.clock.current_speed(session.session_id).await {
                Ok(speed) => speed,
                Err(err) => {
                    error!(%err, "speed lookup failed");
                    break;
                }
            };

            let current_clock = match self.clock.now(session.session_id).await {
                Ok(now) => now,
                Err(err) => {
                    error!(%err, "clock lookup failed");
                    break;
                }
            };

            if trade.event_time.0 <= current_clock.0 {
                previous = TimestampMs(previous.0.max(current_clock.0));
                continue;
            }

            let baseline = if current_clock.0 > previous.0 {
                current_clock
            } else {
                previous
            };

            let delta = trade.event_time.0.saturating_sub(baseline.0);
            if delta > 0 {
                let scaled = (delta as f64) / speed.0;
                let delay = Duration::from_millis(scaled.max(1.0) as u64);
                sleep(delay).await;
            }

            if let Err(err) = self
                .clock
                .advance_to(session.session_id, trade.event_time)
                .await
            {
                error!(%err, "clock advance failed");
                previous = TimestampMs(previous.0.max(current_clock.0));
                continue;
            }

            {
                let mut guard = self.latest_trades.write().await;
                guard.insert((session.session_id, symbol.clone()), trade.clone());
            }

            if let Err(err) = self
                .broadcaster
                .broadcast(session.session_id, serialize_trade(&symbol, &trade))
                .await
            {
                error!(%err, "broadcast failed");
            }

            if let Some(matcher) = &self.spot_matcher {
                matcher.on_trade(session.session_id, &trade).await;
            }

            previous = trade.event_time;
        }

        Ok(())
    }

    async fn collect_klines(
        &self,
        symbol: &str,
        interval: &Interval,
        from: TimestampMs,
        end: TimestampMs,
    ) -> Result<Vec<Kline>, AppError> {
        let mut cursor = from.0.checked_sub(1).unwrap_or(i64::MIN);
        let mut out = Vec::new();

        loop {
            let batch = self
                .market_store
                .get_klines(
                    symbol,
                    interval,
                    Some(TimestampMs(cursor)),
                    Some(end),
                    Some(1000),
                )
                .await?;

            if batch.is_empty() {
                break;
            }

            let last_open = batch.last().map(|k| k.open_time.0).unwrap_or(cursor);
            out.extend(batch.into_iter());

            if last_open >= end.0 {
                break;
            }

            // Evitar repetir el último open_time para no ciclar ni retroceder el reloj
            cursor = last_open.saturating_add(1);
        }

        Ok(out)
    }

    async fn collect_trades(
        &self,
        symbol: &str,
        from: TimestampMs,
        end: TimestampMs,
    ) -> Result<Vec<AggTrade>, AppError> {
        let mut cursor = from.0.checked_sub(1).unwrap_or(i64::MIN);
        let mut out = Vec::new();

        loop {
            let batch = self
                .agg_trades_store
                .get_trades(symbol, Some(TimestampMs(cursor)), Some(end), Some(1000))
                .await?;

            if batch.is_empty() {
                break;
            }

            let last_event = batch.last().map(|t| t.event_time.0).unwrap_or(cursor);
            out.extend(batch.into_iter());

            if last_event >= end.0 {
                break;
            }

            cursor = last_event.saturating_add(1);
        }

        Ok(out)
    }
}

fn serialize_kline(symbol: &str, interval: &Interval, kline: &Kline) -> String {
    let payload = json!({
        "event": "kline",
        "data": {
            "symbol": symbol,
            "interval": interval.as_str(),
            "openTime": kline.open_time.0,
            "closeTime": kline.close_time.0,
            "open": kline.open.0,
            "high": kline.high.0,
            "low": kline.low.0,
            "close": kline.close.0,
            "volume": kline.volume.0
        },
        "stream": format!("kline@{}:{}", interval.as_str(), symbol)
    });
    payload.to_string()
}

fn serialize_trade(symbol: &str, trade: &AggTrade) -> String {
    let payload = json!({
        "event": "trade",
        "data": {
            "symbol": symbol,
            "price": trade.price.to_string(),
            "qty": trade.qty.to_string(),
            "quoteQty": trade.quote_qty.to_string(),
            "isBuyerMaker": trade.is_buyer_maker,
            "eventTime": trade.event_time.0
        },
        "stream": format!("aggTrades:{}", symbol)
    });
    payload.to_string()
}

#[async_trait::async_trait]
impl ReplayEngine for ReplayService {
    async fn start(&self, session: SessionConfig) -> Result<(), AppError> {
        self.cancel_task(session.session_id).await;

        self.clock
            .init_session(session.session_id, session.start_time)
            .await?;
        {
            let mut guard = self.latest_klines.write().await;
            guard.retain(|(id, _), _| *id != session.session_id);
        }
        {
            let mut guard = self.latest_trades.write().await;
            guard.retain(|(id, _), _| *id != session.session_id);
        }

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
        // Obtener el estado actual para decidir si relanzamos o no
        let session = self.sessions_repo.get(session_id).await?;

        // Siempre cancelamos cualquier runner activo para evitar duplicados
        self.cancel_task(session_id).await;

        if session.status != SessionStatus::Running {
            info!(
                session_id = %session_id,
                status = ?session.status,
                "seek requested for non-running session; skipping replay restart"
            );
            let mut guard = self.latest_klines.write().await;
            guard.retain(|(id, _), _| *id != session_id);
            let mut trades_guard = self.latest_trades.write().await;
            trades_guard.retain(|(id, _), _| *id != session_id);
            return Ok(());
        }

        let service = Arc::new(self.clone_inner());
        let handle = tokio::spawn(service.clone().run_session(session.clone(), to));
        self.tasks.write().await.insert(session_id, handle);

        Ok(())
    }

    async fn stop(&self, session_id: Uuid) -> Result<(), AppError> {
        self.cancel_task(session_id).await;
        match self.clock.pause(session_id).await {
            Ok(_) => Ok(()),
            Err(AppError::NotFound(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

impl ReplayService {
    fn clone_inner(&self) -> Self {
        Self {
            market_store: Arc::clone(&self.market_store),
            agg_trades_store: Arc::clone(&self.agg_trades_store),
            clock: Arc::clone(&self.clock),
            sessions_repo: Arc::clone(&self.sessions_repo),
            broadcaster: self.broadcaster.clone(),
            spot_matcher: self.spot_matcher.clone(),
            tasks: Arc::clone(&self.tasks),
            latest_klines: Arc::clone(&self.latest_klines),
            latest_trades: Arc::clone(&self.latest_trades),
        }
    }
}
