use duckdb::{params, Connection};
use uuid::Uuid;

use crate::{
    domain::models::DatasetMetadata,
    error::AppError,
};

fn infer_base_quote(symbol: &str) -> (String, String) {
    const COMMON_QUOTES: [&str; 5] = ["USDT", "USD", "BUSD", "BTC", "ETH"];
    for quote in COMMON_QUOTES.iter() {
        if let Some(base) = symbol.strip_suffix(quote) {
            if !base.is_empty() {
                return (base.to_string(), (*quote).to_string());
            }
        }
    }
    let len = symbol.len();
    let split = len / 2;
    (symbol[..split].to_string(), symbol[split..].to_string())
}

pub fn insert_symbols_if_needed(conn: &Connection, symbol: &str) -> Result<(), AppError> {
    let (base, quote) = infer_base_quote(symbol);
    conn.execute(
        "INSERT OR REPLACE INTO symbols(symbol, base, quote, active) VALUES (?1, ?2, ?3, TRUE)",
        params![symbol, base, quote],
    )
    .map_err(|e| AppError::Database(format!("insert symbol failed: {e}")))?;
    Ok(())
}

/// `rows` es el payload de /api/v3/klines de Binance: Vec<Vec<serde_json::Value>>
/// Retorna el mayor `close_time` insertado.
pub fn insert_klines_chunk(
    conn: &Connection,
    symbol: &str,
    interval: &str,
    rows: &[Vec<serde_json::Value>],
) -> Result<i64, AppError> {
    conn.execute("BEGIN", [])
        .map_err(|e| AppError::Database(format!("begin tx failed: {e}")))?;

    let mut last_close_time: i64 = 0;

    for row in rows {
        // [0] open_time, [1] open, [2] high, [3] low, [4] close, [5] volume, [6] close_time
        let open_time = row
            .get(0)
            .and_then(|v| v.as_i64())
            .ok_or_else(|| AppError::External("missing open_time".to_string()))?;
        let open = row
            .get(1)
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::External("missing open".to_string()))?
            .parse::<f64>()
            .map_err(|e| AppError::External(format!("parse open: {e}")))?;
        let high = row
            .get(2)
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::External("missing high".to_string()))?
            .parse::<f64>()
            .map_err(|e| AppError::External(format!("parse high: {e}")))?;
        let low = row
            .get(3)
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::External("missing low".to_string()))?
            .parse::<f64>()
            .map_err(|e| AppError::External(format!("parse low: {e}")))?;
        let close = row
            .get(4)
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::External("missing close".to_string()))?
            .parse::<f64>()
            .map_err(|e| AppError::External(format!("parse close: {e}")))?;
        let volume = row
            .get(5)
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::External("missing volume".to_string()))?
            .parse::<f64>()
            .map_err(|e| AppError::External(format!("parse volume: {e}")))?;
        let close_time = row
            .get(6)
            .and_then(|v| v.as_i64())
            .ok_or_else(|| AppError::External("missing close_time".to_string()))?;

        conn.execute(
            "INSERT OR REPLACE INTO klines(
                symbol, interval, open_time, open, high, low, close, volume, close_time
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                symbol,
                interval,
                open_time,
                open,
                high,
                low,
                close,
                volume,
                close_time
            ],
        )
        .map_err(|e| AppError::Database(format!("insert kline failed: {e}")))?;

        if close_time > last_close_time {
            last_close_time = close_time;
        }
    }

    conn.execute("COMMIT", [])
        .map_err(|e| AppError::Database(format!("commit tx failed: {e}")))?;

    Ok(last_close_time)
}

pub fn mark_dataset_status(conn: &Connection, id: Uuid, status: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE datasets SET status = ?1 WHERE id = ?2",
        params![status, id.to_string()],
    )
    .map_err(|e| AppError::Database(format!("update status failed: {e}")))?;
    Ok(())
}

pub fn select_dataset_meta(
    conn: &Connection,
    id: Uuid,
) -> Result<(String, String, i64, i64), AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT symbol, interval, start_time, end_time
             FROM datasets WHERE id = ?1",
        )
        .map_err(|e| AppError::Database(format!("prepare dataset lookup failed: {e}")))?;
    let mut rows = stmt
        .query(params![id.to_string()])
        .map_err(|e| AppError::Database(format!("query dataset failed: {e}")))?;
    let row = rows
        .next()
        .map_err(|e| AppError::Database(format!("read dataset row failed: {e}")))?
        .ok_or_else(|| AppError::NotFound(format!("dataset {id} not found")))?;

    let symbol: String = row
        .get(0)
        .map_err(|e| AppError::Database(format!("column error: {e}")))?;
    let interval: String = row
        .get(1)
        .map_err(|e| AppError::Database(format!("column error: {e}")))?;
    let start_time: i64 = row
        .get(2)
        .map_err(|e| AppError::Database(format!("column error: {e}")))?;
    let end_time: i64 = row
        .get(3)
        .map_err(|e| AppError::Database(format!("column error: {e}")))?;
    Ok((symbol, interval, start_time, end_time))
}

pub fn insert_dataset_row(conn: &Connection, meta: &DatasetMetadata) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO datasets(
            id, symbol, interval, start_time, end_time, source, status, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, 'binance', ?6, ?7)",
        params![
            meta.id.to_string(),
            meta.symbol,
            meta.interval,
            meta.start_time,
            meta.end_time,
            meta.status,
            meta.created_at
        ],
    )
    .map_err(|e| AppError::Database(format!("insert dataset failed: {e}")))?;
    Ok(())
}

pub fn list_datasets_query(conn: &Connection) -> Result<Vec<DatasetMetadata>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, symbol, interval, start_time, end_time, status, created_at
             FROM datasets
             ORDER BY created_at DESC",
        )
        .map_err(|e| AppError::Database(format!("prepare datasets failed: {e}")))?;
    let mut rows = stmt
        .query([])
        .map_err(|e| AppError::Database(format!("query datasets failed: {e}")))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| AppError::Database(format!("row iteration failed: {e}")))?
    {
        let id_str: String = row
            .get(0)
            .map_err(|e| AppError::Database(format!("column error: {e}")))?;
        let id = Uuid::parse_str(&id_str)
            .map_err(|e| AppError::Database(format!("uuid parse error: {e}")))?;
        out.push(DatasetMetadata {
            id,
            symbol: row
                .get::<_, String>(1)
                .map_err(|e| AppError::Database(format!("column error: {e}")))?,
            interval: row
                .get::<_, String>(2)
                .map_err(|e| AppError::Database(format!("column error: {e}")))?,
            start_time: row
                .get::<_, i64>(3)
                .map_err(|e| AppError::Database(format!("column error: {e}")))?,
            end_time: row
                .get::<_, i64>(4)
                .map_err(|e| AppError::Database(format!("column error: {e}")))?,
            status: row
                .get::<_, String>(5)
                .map_err(|e| AppError::Database(format!("column error: {e}")))?,
            created_at: row
                .get::<_, i64>(6)
                .map_err(|e| AppError::Database(format!("column error: {e}")))?,
        });
    }
    Ok(out)
}
