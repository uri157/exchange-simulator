use reqwest::Client;
use serde_json::Value;

use crate::{
    domain::models::DatasetMetadata,
    error::AppError,
};

use super::{db::DuckDbPool, ingest_sql};

/// Orquesta la descarga en chunks (máx 1000) desde Binance y la ingesta en DuckDB.
/// Devuelve `true` si insertó al menos una fila.
pub async fn run_ingest(pool: &DuckDbPool, meta: &DatasetMetadata) -> Result<bool, AppError> {
    let client = Client::builder()
        .user_agent("exchange-simulator/ingest")
        .build()
        .map_err(|e| AppError::Internal(format!("http client build failed: {e}")))?;

    let base = "https://api.binance.com/api/v3/klines";
    let symbol = &meta.symbol;
    let interval = &meta.interval;

    let mut from = meta.start_time;
    let end_time = meta.end_time;
    let mut any_inserted = false;

    while from < end_time {
        let url = format!(
            "{base}?symbol={symbol}&interval={interval}&startTime={from}&endTime={end_time}&limit=1000"
        );

        let resp = client
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

        // Inserta el chunk dentro de una conexión corta (sin await dentro).
        let last_close = pool
            .with_conn_async({
                let symbol = symbol.clone();
                let interval = interval.clone();
                let chunk = chunk; // mueve chunk al closure
                move |conn| {
                    ingest_sql::insert_symbols_if_needed(conn, &symbol)?;
                    let last = ingest_sql::insert_klines_chunk(conn, &symbol, &interval, &chunk)?;
                    Ok::<i64, AppError>(last)
                }
            })
            .await?;

        any_inserted = true;

        // Evita loops en intervalos sin avance.
        if last_close <= from {
            break;
        }
        from = last_close + 1;
    }

    Ok(any_inserted)
}
