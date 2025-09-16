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
                out.push(Symbol {
                    symbol: row
                        .get(0)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?,
                    base: row
                        .get(1)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?,
                    quote: row
                        .get(2)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?,
                    active: row
                        .get(3)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?,
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
                out.push(Kline {
                    symbol: row
                        .get(0)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?,
                    interval: Interval::new(row
                        .get::<_, String>(1)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?),
                    open_time: TimestampMs(row
                        .get::<_, i64>(2)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?),
                    open: Price(row
                        .get::<_, f64>(3)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?),
                    high: Price(row
                        .get::<_, f64>(4)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?),
                    low: Price(row
                        .get::<_, f64>(5)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?),
                    close: Price(row
                        .get::<_, f64>(6)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?),
                    volume: Quantity(row
                        .get::<_, f64>(7)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?),
                    close_time: TimestampMs(row
                        .get::<_, i64>(8)
                        .map_err(|err| AppError::Database(format!("row column error: {err}")))?),
                });
            }
            Ok(out)
        })
        .await
    }
}
