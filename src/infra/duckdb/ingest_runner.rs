use duckdb::Connection;
use reqwest::Client;
use serde_json::Value;

use crate::error::AppError;

use super::ingest_sql;

/// Descarga e ingesta velas de Binance en chunks de hasta 1000 filas.
/// Avanza desde `start_time` hasta `end_time` (ms epoch).
pub async fn download_and_ingest(
    conn: &Connection,
    http: &Client,
    symbol: &str,
    interval: &str,
    start_time: i64,
    end_time: i64,
) -> Result<(), AppError> {
    let base = "https://api.binance.com/api/v3/klines";
    let mut from = start_time;
    let mut any_inserted = false;

    while from < end_time {
        let url = format!(
            "{base}?symbol={symbol}&interval={interval}&startTime={from}&endTime={end_time}&limit=1000"
        );

        let resp = http
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::External(format!("binance request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::External(format!(
                "binance status {} for {}",
                resp.status(),
                url
            )));
        }

        let chunk: Vec<Vec<Value>> = resp
            .json()
            .await
            .map_err(|e| AppError::External(format!("binance parse failed: {e}")))?;

        if chunk.is_empty() {
            break;
        }

        // Asegura símbolo y mete el chunk.
        ingest_sql::insert_symbols_if_needed(conn, symbol)?;
        let last_close = ingest_sql::insert_klines_chunk(conn, symbol, interval, &chunk)?;

        any_inserted = true;

        // Evita loops en intervalos sin avance.
        if last_close <= from {
            break;
        }
        from = last_close + 1;
    }

    // Si no se insertó nada, no es error aquí: el caller decide marcar "failed" o similar.
    if !any_inserted {
        // No-op, dejar que el caller decida.
    }

    Ok(())
}
