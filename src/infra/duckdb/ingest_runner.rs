use reqwest::Client;
use serde_json::Value;
use tracing::info;

use crate::{domain::models::DatasetMetadata, error::AppError};

use super::{db::DuckDbPool, ingest_sql};

/// Ejecuta el proceso de descarga desde Binance y almacenamiento en DuckDB.
/// Además, registra progreso en `dataset_progress`.
/// Retorna `true` si se insertó al menos un kline.
pub async fn run_ingest(pool: &DuckDbPool, meta: &DatasetMetadata) -> Result<bool, AppError> {
    let client = Client::builder()
        .user_agent("exchange-simulator/ingest-runner")
        .build()
        .map_err(|e| AppError::External(format!("reqwest client build failed: {e}")))?;

    let base = "https://api.binance.com/api/v3/klines";
    let mut from = meta.start_time;
    let mut inserted_any = false;

    // Estimación de total esperado (aprox) según interval
    let interval_ms = interval_to_ms(&meta.interval)
        .ok_or_else(|| AppError::Validation(format!("unsupported interval '{}'", meta.interval)))?;
    let total_est = ((meta.end_time - meta.start_time).max(0) / interval_ms as i64).max(0);

    // Inicializa progreso
    {
        let pool2 = pool.clone();
        let meta_id = meta.id;
        pool2
            .with_conn_async(move |conn| {
                let now = chrono::Utc::now().timestamp_millis();
                ingest_sql::progress_init(conn, meta_id, total_est, now)
            })
            .await?;
    }

    let mut accumulated_inserted: i64 = 0;
    let mut last_logged_pct: i64 = -1;

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

        // Inserta el batch y obtiene (último close_time, filas afectadas)
        let (last_close, inserted_count) = pool
            .with_conn_async(move |conn| {
                ingest_sql::insert_symbols_if_needed(conn, &sym)?;
                let (last, affected) =
                    ingest_sql::insert_klines_chunk(conn, &sym, &intv, &chunk)?;
                Ok::<(i64, i64), AppError>((last, affected))
            })
            .await?;

        if inserted_count > 0 {
            inserted_any = true;
        }
        accumulated_inserted += inserted_count;

        // Actualiza progreso
        {
            let pool2 = pool.clone();
            let dataset_id = meta.id;
            pool2
                .with_conn_async(move |conn| {
                    let now = chrono::Utc::now().timestamp_millis();
                    ingest_sql::progress_upsert_chunk(
                        conn,
                        dataset_id,
                        inserted_count,
                        last_close,
                        now,
                    )
                })
                .await?;
        }

        // Logging de progreso (cada 5%)
        if total_est > 0 {
            let pct = ((accumulated_inserted * 100) / total_est).clamp(0, 100);
            if pct >= last_logged_pct + 5 {
                info!(
                    dataset_id = %meta.id,
                    symbol = %meta.symbol,
                    interval = %meta.interval,
                    inserted = accumulated_inserted,
                    total_est = total_est,
                    "{pct}% approx ({accumulated_inserted}/{total_est})"
                );
                last_logged_pct = pct;
            }
        } else {
            // Sin estimación útil, log por batch
            info!(
                dataset_id = %meta.id,
                symbol = %meta.symbol,
                interval = %meta.interval,
                inserted_batch = inserted_count,
                inserted_total = accumulated_inserted,
                "ingest progress"
            );
        }

        if last_close <= from {
            break;
        }
        from = last_close + 1;
    }

    Ok(inserted_any)
}

/// Mapea el string de intervalo de Binance a milisegundos
fn interval_to_ms(s: &str) -> Option<u64> {
    // m=min, h=hour, d=day, w=week, M=month(aprox 30d para estimación)
    let mult = |n: u64, unit_ms: u64| Some(n.saturating_mul(unit_ms));
    match s {
        "1m" => mult(1, 60_000),
        "3m" => mult(3, 60_000),
        "5m" => mult(5, 60_000),
        "15m" => mult(15, 60_000),
        "30m" => mult(30, 60_000),
        "1h" => mult(1, 3_600_000),
        "2h" => mult(2, 3_600_000),
        "4h" => mult(4, 3_600_000),
        "6h" => mult(6, 3_600_000),
        "8h" => mult(8, 3_600_000),
        "12h" => mult(12, 3_600_000),
        "1d" => mult(1, 86_400_000),
        "3d" => mult(3, 86_400_000),
        "1w" => mult(7, 86_400_000),
        "1M" => mult(30, 86_400_000), // aprox
        _ => None,
    }
}
