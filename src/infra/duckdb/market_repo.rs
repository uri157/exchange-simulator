use duckdb::params;

use crate::{
    domain::{
        models::{Kline, Symbol},
        traits::MarketStore,
        value_objects::{Interval, Price, Quantity, TimestampMs},
    },
    error::AppError,
};

use super::db::DuckDbPool;

pub struct DuckDbMarketStore {
    pool: DuckDbPool,
}

impl DuckDbMarketStore {
    pub fn new(pool: DuckDbPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl MarketStore for DuckDbMarketStore {
    async fn list_symbols(&self) -> Result<Vec<Symbol>, AppError> {
        let pool = self.pool.clone();
        pool.with_conn_async(|conn| {
            let mut stmt = conn
                .prepare("SELECT symbol, base, quote, active FROM symbols ORDER BY symbol")
                .map_err(|err| AppError::Database(format!("query symbols failed: {err}")))?;
            let mut rows = stmt
                .query([])
                .map_err(|err| AppError::Database(format!("query symbols failed: {err}")))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|err| AppError::Database(format!("rows iteration failed: {err}")))?
            {
                let symbol: String = row
                    .get(0)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;
                let base: String = row
                    .get(1)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;
                let quote: String = row
                    .get(2)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;
                let active: bool = row
                    .get(3)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;

                out.push(Symbol {
                    symbol,
                    base,
                    quote,
                    active,
                });
            }
            Ok(out)
        })
        .await
    }

    async fn get_klines(
        &self,
        symbol: &str,
        interval: &Interval,
        start: Option<TimestampMs>,
        end: Option<TimestampMs>,
        limit: Option<usize>,
    ) -> Result<Vec<Kline>, AppError> {
        let pool = self.pool.clone();
        let symbol = symbol.to_string();
        let interval = interval.clone();
        pool.with_conn_async(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT symbol, interval, open_time, open, high, low, close, volume, close_time \
                     FROM klines WHERE symbol = ?1 AND interval = ?2 \
                     AND (?3 IS NULL OR open_time >= ?3) \
                     AND (?4 IS NULL OR open_time <= ?4) \
                     ORDER BY open_time LIMIT ?5",
                )
                .map_err(|err| AppError::Database(format!("prepare klines failed: {err}")))?;
            let limit = limit.unwrap_or(500) as i64;
            let start_val: Option<i64> = start.map(|s| s.0);
            let end_val: Option<i64> = end.map(|s| s.0);
            let mut rows = stmt
                .query(params![symbol, interval.as_str(), start_val, end_val, limit])
                .map_err(|err| AppError::Database(format!("query klines failed: {err}")))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|err| AppError::Database(format!("row iteration failed: {err}")))?
            {
                let symbol: String = row
                    .get(0)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;
                let interval_str: String = row
                    .get(1)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;
                let open_time: i64 = row
                    .get(2)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;
                let open: f64 = row
                    .get(3)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;
                let high: f64 = row
                    .get(4)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;
                let low: f64 = row
                    .get(5)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;
                let close: f64 = row
                    .get(6)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;
                let volume: f64 = row
                    .get(7)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;
                let close_time: i64 = row
                    .get(8)
                    .map_err(|err| AppError::Database(format!("row column error: {err}")))?;

                out.push(Kline {
                    symbol,
                    interval: Interval::new(&interval_str),
                    open_time: TimestampMs(open_time),
                    open: Price(open),
                    high: Price(high),
                    low: Price(low),
                    close: Price(close),
                    volume: Quantity(volume),
                    close_time: TimestampMs(close_time),
                });
            }
            Ok(out)
        })
        .await
    }
}
