use reqwest::Client;
use serde_json::Value;

use crate::{domain::models::DatasetMetadata, error::AppError};

use super::{db::DuckDbPool, ingest_sql};

/// Ejecuta el proceso de descarga desde Binance y almacenamiento en DuckDB.
/// Retorna `true` si se insertó al menos un kline.
pub async fn run_ingest(pool: &DuckDbPool, meta: &DatasetMetadata) -> Result<bool, AppError> {
    let client = Client::builder()
        .user_agent("exchange-simulator/ingest-runner")
        .build()
        .map_err(|e| AppError::External(format!("reqwest client build failed: {e}")))?;

    let base = "https://api.binance.com/api/v3/klines";
    let mut from = meta.start_time;
    let mut inserted_any = false;

    while from < meta.end_time {
        let url = format!(
            "{base}?symbol={}&interval={}&startTime={from}&endTime={}&limit=1000",
            meta.symbol, meta.interval, meta.end_time
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

        let sym = meta.symbol.clone();
        let intv = meta.interval.clone();

        let last_close = pool
            .with_conn_async(move |conn| {
                ingest_sql::insert_symbols_if_needed(conn, &sym)?;
                let last = ingest_sql::insert_klines_chunk(conn, &sym, &intv, &chunk)?;
                Ok::<i64, AppError>(last)
            })
            .await?;

        inserted_any = true;

        if last_close <= from {
            break;
        }
        from = last_close + 1;
    }

    Ok(inserted_any)
}
