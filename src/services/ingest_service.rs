use std::sync::Arc;

use rand::{rngs::StdRng, Rng, SeedableRng};
use tracing::info;
use uuid::Uuid;

use super::ServiceResult;
use crate::{
    domain::{
        models::{AggTrade, DatasetMetadata, Kline},
        traits::{MarketIngestor, MarketStore},
        value_objects::{Interval, TimestampMs},
    },
    error::AppError,
    infra::{binance::BinanceClient, duckdb::agg_trades_repo::DuckDbAggTradesStore},
};

#[derive(Clone)]
pub struct IngestService {
    ingestor: Arc<dyn MarketIngestor>,
    market_store: Arc<dyn MarketStore>,
    agg_trades_store: Arc<DuckDbAggTradesStore>,
    binance_client: Arc<BinanceClient>,
}

#[derive(Debug, Clone, Default)]
pub struct IngestAggTradesResult {
    pub symbol: String,
    pub fetched: usize,
    pub inserted: usize,
    pub skipped: usize,
    pub from_id_start: Option<i64>,
    pub from_id_end: Option<i64>,
    pub t_first: Option<i64>,
    pub t_last: Option<i64>,
}

impl IngestService {
    pub fn new(
        ingestor: Arc<dyn MarketIngestor>,
        market_store: Arc<dyn MarketStore>,
        agg_trades_store: Arc<DuckDbAggTradesStore>,
        binance_client: Arc<BinanceClient>,
    ) -> Self {
        Self {
            ingestor,
            market_store,
            agg_trades_store,
            binance_client,
        }
    }

    pub async fn register_dataset(
        &self,
        symbol: &str,
        interval: &str,
        start_time: i64,
        end_time: i64,
    ) -> ServiceResult<DatasetMetadata> {
        self.ingestor
            .register_dataset(symbol, interval, start_time, end_time)
            .await
    }

    pub async fn list_datasets(&self) -> ServiceResult<Vec<DatasetMetadata>> {
        self.ingestor.list_datasets().await
    }

    pub async fn ingest_dataset(&self, dataset_id: Uuid) -> ServiceResult<()> {
        self.ingestor.ingest_dataset(dataset_id).await
    }

    pub async fn list_ready_symbols(&self) -> ServiceResult<Vec<String>> {
        self.ingestor.list_ready_symbols().await
    }

    pub async fn list_ready_intervals(&self, symbol: &str) -> ServiceResult<Vec<String>> {
        self.ingestor.list_ready_intervals(symbol).await
    }

    pub async fn get_range(&self, symbol: &str, interval: &str) -> ServiceResult<(i64, i64)> {
        self.ingestor.get_range(symbol, interval).await
    }

    pub async fn ingest_agg_trades(
        &self,
        symbol: String,
        start_time: Option<i64>,
        end_time: Option<i64>,
        clear_before: bool,
    ) -> Result<IngestAggTradesResult, AppError> {
        let symbol_clean = symbol.trim().to_uppercase();
        if symbol_clean.is_empty() {
            return Err(AppError::Validation("symbol cannot be empty".into()));
        }

        if let (Some(start), Some(end)) = (start_time, end_time) {
            if start > end {
                return Err(AppError::Validation(
                    "start_time cannot be greater than end_time".into(),
                ));
            }
        }

        if clear_before && (start_time.is_none() || end_time.is_none()) {
            return Err(AppError::Validation(
                "clear_before requires start_time and end_time".into(),
            ));
        }

        let mut result = IngestAggTradesResult {
            symbol: symbol_clean.clone(),
            ..Default::default()
        };

        if clear_before {
            if let (Some(start), Some(end)) = (start_time, end_time) {
                let removed = self
                    .agg_trades_store
                    .clear_range(&symbol_clean, TimestampMs(start), TimestampMs(end))
                    .await?;
                info!(
                    symbol = %symbol_clean,
                    removed,
                    start_time = start,
                    end_time = end,
                    "cleared agg trades range before ingestion"
                );
            }
        }

        let existing_max = self.agg_trades_store.max_trade_id(&symbol_clean).await?;

        let mut dedupe_cutoff = if clear_before { None } else { existing_max };
        let mut from_id_cursor = if clear_before {
            None
        } else {
            existing_max.map(|id| id + 1)
        };
        let mut start_time_cursor = if from_id_cursor.is_some() {
            None
        } else {
            start_time
        };

        if from_id_cursor.is_none() && start_time_cursor.is_none() {
            return Err(AppError::Validation(
                "start_time is required when there are no existing trades".into(),
            ));
        }

        let mut total_fetched = 0usize;
        let mut total_inserted = 0usize;
        let mut total_skipped = 0usize;
        let mut reached_end = false;

        while !reached_end {
            let page = self
                .binance_client
                .get_agg_trades(
                    &symbol_clean,
                    start_time_cursor,
                    end_time,
                    from_id_cursor,
                    Some(1000),
                )
                .await?;

            if page.is_empty() {
                break;
            }

            let page_len = page.len();
            total_fetched += page_len;

            let last_remote_id = page.last().map(|t| t.a);
            let last_remote_time = page.last().map(|t| t.T);

            let mut normalized = Vec::with_capacity(page_len);
            for trade in page.iter() {
                if let Some(end) = end_time {
                    if trade.T > end {
                        reached_end = true;
                        break;
                    }
                }

                if let Some(cutoff) = dedupe_cutoff {
                    if trade.a <= cutoff {
                        continue;
                    }
                }

                normalized.push(AggTrade {
                    symbol: symbol_clean.clone(),
                    event_time: TimestampMs(trade.T),
                    trade_id: trade.a,
                    price: trade.p,
                    qty: trade.q,
                    quote_qty: trade.Q,
                    is_buyer_maker: trade.m,
                });

                if result.from_id_start.is_none() {
                    result.from_id_start = Some(trade.a);
                    result.t_first = Some(trade.T);
                }
                result.from_id_end = Some(trade.a);
                result.t_last = Some(trade.T);

                dedupe_cutoff = Some(trade.a);
            }

            let inserted_now = self.agg_trades_store.insert_trades(&normalized).await?;
            total_inserted += inserted_now;
            total_skipped += page_len.saturating_sub(inserted_now);

            if let (Some(last_id), Some(last_time)) = (last_remote_id, last_remote_time) {
                info!(
                    symbol = %symbol_clean,
                    fetched = page_len,
                    last_trade_id = last_id,
                    last_trade_time = last_time,
                    "fetched aggTrades page"
                );
                from_id_cursor = Some(last_id + 1);
                start_time_cursor = None;
            }

            if reached_end {
                break;
            }
        }

        result.fetched = total_fetched;
        result.inserted = total_inserted;
        result.skipped = total_skipped;

        Ok(result)
    }

    pub async fn seed_aggtrades_from_klines(
        &self,
        symbol: &str,
        interval: Interval,
        from: TimestampMs,
        to: TimestampMs,
        trades_per_kline: usize,
        seed: u64,
    ) -> ServiceResult<u64> {
        if symbol.trim().is_empty() {
            return Err(AppError::Validation("symbol cannot be empty".into()));
        }
        if trades_per_kline == 0 {
            return Err(AppError::Validation(
                "trades_per_kline must be greater than zero".into(),
            ));
        }
        if from.0 >= to.0 {
            return Err(AppError::Validation("from must be before to".into()));
        }

        let mut klines = Vec::new();
        let mut cursor = from.0.checked_sub(1).map(TimestampMs);

        loop {
            let batch = self
                .market_store
                .get_klines(symbol, &interval, cursor, Some(to), Some(1000))
                .await?;

            if batch.is_empty() {
                break;
            }

            cursor = batch.last().map(|k| k.open_time);
            append_in_range(&mut klines, batch, from, to);

            if let Some(last) = cursor {
                if last.0 >= to.0 {
                    break;
                }
            } else {
                break;
            }
        }

        if klines.is_empty() {
            return Err(AppError::NotFound(format!(
                "no klines available for {symbol} {} in selected range",
                interval.as_str()
            )));
        }

        // Remove any existing trades in the requested window.
        self.agg_trades_store.clear_range(symbol, from, to).await?;

        let mut next_trade_id = self
            .agg_trades_store
            .max_trade_id(symbol)
            .await?
            .unwrap_or(-1)
            + 1;

        let mut rng = StdRng::seed_from_u64(seed);
        let mut trades = Vec::with_capacity(klines.len() * trades_per_kline);
        let symbol_owned = symbol.to_string();

        for kline in klines {
            let span = kline.close_time.0.saturating_sub(kline.open_time.0).max(1);
            let base_step = span as f64 / (trades_per_kline as f64 + 1.0);
            let price_delta = kline.close.0 - kline.open.0;
            let price_range = (kline.high.0 - kline.low.0).abs();
            let qty_base = (price_range / trades_per_kline as f64).max(0.0001);

            for trade_index in 0..trades_per_kline {
                let position = (trade_index + 1) as f64 / (trades_per_kline as f64 + 1.0);
                let jitter = (rng.gen::<f64>() - 0.5) * base_step * 0.3;
                let mut event_time =
                    (kline.open_time.0 as f64 + position * span as f64 + jitter).round() as i64;
                let lower_bound = if kline.close_time.0 > kline.open_time.0 {
                    kline.open_time.0 + 1
                } else {
                    kline.close_time.0
                };
                let upper_bound = kline.close_time.0;
                event_time = event_time.clamp(lower_bound, upper_bound);

                let price_base = kline.open.0 + price_delta * position;
                let price_jitter = (rng.gen::<f64>() - 0.5) * price_range * 0.05;
                let mut price = price_base + price_jitter;
                if price <= 0.0 {
                    price = price_base.max(0.0001);
                }

                let qty = (qty_base * (0.8 + rng.gen::<f64>() * 0.4)).max(0.0001);
                let quote_qty = price * qty;
                let is_buyer_maker = next_trade_id % 2 == 0;

                trades.push(AggTrade {
                    symbol: symbol_owned.clone(),
                    event_time: TimestampMs(event_time),
                    trade_id: next_trade_id,
                    price,
                    qty,
                    quote_qty,
                    is_buyer_maker,
                });

                next_trade_id += 1;
            }
        }

        trades.sort_by(|a, b| {
            a.event_time
                .0
                .cmp(&b.event_time.0)
                .then(a.trade_id.cmp(&b.trade_id))
        });

        let inserted = self.agg_trades_store.insert_trades(&trades).await? as u64;

        Ok(inserted)
    }
}

fn append_in_range(acc: &mut Vec<Kline>, batch: Vec<Kline>, from: TimestampMs, to: TimestampMs) {
    for kline in batch {
        if kline.open_time.0 >= from.0 && kline.open_time.0 <= to.0 {
            acc.push(kline);
        }
    }
}
