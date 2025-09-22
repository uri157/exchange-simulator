use duckdb::params;

use crate::{
    domain::{models::AggTrade, traits::AggTradesStore, value_objects::TimestampMs},
    error::AppError,
};

use super::db::DuckDbPool;

pub struct DuckDbAggTradesStore {
    pool: DuckDbPool,
}

impl DuckDbAggTradesStore {
    pub fn new(pool: DuckDbPool) -> Self {
        Self { pool }
    }

    fn normalize_limit(limit: Option<usize>) -> usize {
        limit.unwrap_or(1000).clamp(1, 5000)
    }

    pub async fn clear_range(
        &self,
        symbol: &str,
        from: TimestampMs,
        to: TimestampMs,
    ) -> Result<usize, AppError> {
        if symbol.trim().is_empty() {
            return Err(AppError::Validation("symbol cannot be empty".into()));
        }
        if from.0 > to.0 {
            return Err(AppError::Validation(
                "from timestamp cannot be greater than to".into(),
            ));
        }

        let pool = self.pool.clone();
        let symbol = symbol.to_string();
        pool.with_conn_async(move |conn| {
            let mut stmt = conn
                .prepare(
                    "DELETE FROM agg_trades
                     WHERE symbol = ?1 AND event_time >= ?2 AND event_time <= ?3",
                )
                .map_err(|err| AppError::Database(format!("prepare delete agg trades: {err}")))?;
            let affected = stmt
                .execute(params![symbol, from.0, to.0])
                .map_err(|err| AppError::Database(format!("delete agg trades: {err}")))?;
            Ok(affected)
        })
        .await
    }

    pub async fn max_trade_id(&self, symbol: &str) -> Result<Option<i64>, AppError> {
        if symbol.trim().is_empty() {
            return Err(AppError::Validation("symbol cannot be empty".into()));
        }

        let pool = self.pool.clone();
        let symbol = symbol.to_string();
        pool.with_conn_async(move |conn| {
            let mut stmt = conn
                .prepare("SELECT MAX(trade_id) FROM agg_trades WHERE symbol = ?1")
                .map_err(|err| AppError::Database(format!("prepare max trade_id: {err}")))?;
            let mut rows = stmt
                .query(params![symbol])
                .map_err(|err| AppError::Database(format!("query max trade_id: {err}")))?;
            let row = rows
                .next()
                .map_err(|err| AppError::Database(format!("iterate max trade_id: {err}")))?
                .ok_or_else(|| AppError::Database("no row returned for max trade_id".into()))?;
            let max_val: Option<i64> = row
                .get(0)
                .map_err(|err| AppError::Database(format!("read max trade_id: {err}")))?;
            Ok(max_val)
        })
        .await
    }

    pub async fn insert_trades(&self, trades: &[AggTrade]) -> Result<usize, AppError> {
        if trades.is_empty() {
            return Ok(0);
        }

        let mut records: Vec<AggTrade> = trades.to_vec();
        records.sort_by(|a, b| {
            a.event_time
                .0
                .cmp(&b.event_time.0)
                .then(a.trade_id.cmp(&b.trade_id))
        });

        let pool = self.pool.clone();
        pool.with_conn_async(move |conn| {
            // Manejo de transacción sin &mut Connection: BEGIN/COMMIT manuales
            conn.execute("BEGIN", [])
                .map_err(|err| AppError::Database(format!("begin agg trades tx: {err}")))?;

            let result: Result<usize, AppError> = (|| {
                let mut stmt = conn
                    .prepare(
                        "INSERT INTO agg_trades (
                            symbol, event_time, trade_id, price, qty, quote_qty, is_buyer_maker
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    )
                    .map_err(|err| {
                        AppError::Database(format!("prepare insert agg trades: {err}"))
                    })?;
                for trade in &records {
                    stmt.execute(params![
                        &trade.symbol,
                        trade.event_time.0,
                        trade.trade_id,
                        trade.price,
                        trade.qty,
                        trade.quote_qty,
                        trade.is_buyer_maker
                    ])
                    .map_err(|err| AppError::Database(format!("insert agg trade failed: {err}")))?;
                }
                Ok(records.len())
            })();

            match result {
                Ok(n) => {
                    conn.execute("COMMIT", []).map_err(|err| {
                        AppError::Database(format!("commit agg trades tx: {err}"))
                    })?;
                    Ok(n)
                }
                Err(e) => {
                    let _ = conn.execute("ROLLBACK", []);
                    Err(e)
                }
            }
        })
        .await
    }
}

#[async_trait::async_trait]
impl AggTradesStore for DuckDbAggTradesStore {
    async fn get_trades(
        &self,
        symbol: &str,
        from: Option<TimestampMs>,
        to: Option<TimestampMs>,
        limit: Option<usize>,
    ) -> Result<Vec<AggTrade>, AppError> {
        if symbol.trim().is_empty() {
            return Err(AppError::Validation("symbol cannot be empty".into()));
        }

        let limit = Self::normalize_limit(limit);
        let pool = self.pool.clone();
        let symbol = symbol.to_string();
        let from_val = from.map(|ts| ts.0);
        let to_val = to.map(|ts| ts.0);

        pool.with_conn_async(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT symbol, event_time, trade_id, price, qty, quote_qty, is_buyer_maker
                     FROM agg_trades
                     WHERE symbol = ?1
                       AND (?2 IS NULL OR event_time > ?2)
                       AND (?3 IS NULL OR event_time <= ?3)
                     ORDER BY event_time ASC, trade_id ASC
                     LIMIT ?4",
                )
                .map_err(|err| AppError::Database(format!("prepare agg trades query: {err}")))?;

            let mut rows = stmt
                .query(params![symbol, from_val, to_val, limit as i64])
                .map_err(|err| AppError::Database(format!("query agg trades: {err}")))?;

            let mut trades = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|err| AppError::Database(format!("iterate agg trades: {err}")))?
            {
                let symbol: String = row
                    .get(0)
                    .map_err(|err| AppError::Database(format!("read symbol: {err}")))?;
                let event_time: i64 = row
                    .get(1)
                    .map_err(|err| AppError::Database(format!("read event_time: {err}")))?;
                let trade_id: i64 = row
                    .get(2)
                    .map_err(|err| AppError::Database(format!("read trade_id: {err}")))?;
                let price: f64 = row
                    .get(3)
                    .map_err(|err| AppError::Database(format!("read price: {err}")))?;
                let qty: f64 = row
                    .get(4)
                    .map_err(|err| AppError::Database(format!("read qty: {err}")))?;
                let quote_qty: f64 = row
                    .get(5)
                    .map_err(|err| AppError::Database(format!("read quote_qty: {err}")))?;
                let is_buyer_maker: bool = row
                    .get(6)
                    .map_err(|err| AppError::Database(format!("read is_buyer_maker: {err}")))?;

                trades.push(AggTrade {
                    symbol,
                    event_time: TimestampMs(event_time),
                    trade_id,
                    price,
                    qty,
                    quote_qty,
                    is_buyer_maker,
                });
            }

            Ok(trades)
        })
        .await
    }
}
