use reqwest::Client;
use serde_json::Value;
use tracing::info;

use crate::{
    domain::models::DatasetMetadata,
    error::AppError,
    infra::ingest::progress_sink::ProgressSink,
};

use super::{db::DuckDbPool, ingest_sql};

/// Resultado de la ejecución de ingesta.
pub enum IngestOutcome {
    Completed { inserted_any: bool },
    Canceled,
}

/// Ejecuta el proceso de descarga desde Binance y almacenamiento en DuckDB.
/// Además, registra progreso en `dataset_progress`.
pub async fn run_ingest<S>(
    pool: &DuckDbPool,
    meta: &DatasetMetadata,
    sink: &S,
) -> Result<IngestOutcome, AppError>
where
    S: ProgressSink + ?Sized,
{
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

    // Inicializa progreso persistido
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

    sink.set_progress(0, Some("Starting ingestion".to_string()));
    sink.append_log(format!(
        "starting ingestion for {} {} ({} -> {})",
        meta.symbol, meta.interval, meta.start_time, meta.end_time
    ));

    if sink.is_cancelled() {
        sink.append_log("ingestion canceled before start".to_string());
        return Ok(IngestOutcome::Canceled);
    }

    let mut accumulated_inserted: i64 = 0;
    let mut last_logged_pct: i64 = -1;

    while from < meta.end_time {
        if sink.is_cancelled() {
            sink.append_log("ingestion canceled".to_string());
            return Ok(IngestOutcome::Canceled);
        }

        let url = format!(
            "{base}?symbol={}&interval={}&startTime={from}&endTime={}&limit=1000",
            meta.symbol, meta.interval, meta.end_time
        );

        let resp = client.get(&url).send().await.map_err(|e| {
            let msg = format!("binance request failed: {e}");
            sink.append_log(msg.clone());
            AppError::External(msg)
        })?;

        if !resp.status().is_success() {
            let msg = format!("binance status {} for {}", resp.status(), url);
            sink.append_log(msg.clone());
            return Err(AppError::External(msg));
        }

        let chunk: Vec<Vec<Value>> = resp.json().await.map_err(|e| {
            let msg = format!("binance parse failed: {e}");
            sink.append_log(msg.clone());
            AppError::External(msg)
        })?;

        if chunk.is_empty() {
            break;
        }

        let sym = meta.symbol.clone();
        let intv = meta.interval.clone();

        // Inserta el batch y obtiene (último close_time, filas afectadas)
        let (last_close, inserted_count) = pool
            .with_conn_async(move |conn| {
                ingest_sql::insert_symbols_if_needed(conn, &sym)?;
                let (last, affected) = ingest_sql::insert_klines_chunk(conn, &sym, &intv, &chunk)?;
                Ok::<(i64, i64), AppError>((last, affected))
            })
            .await?;

        if inserted_count > 0 {
            inserted_any = true;
        }
        accumulated_inserted += inserted_count;

        // Actualiza progreso persistido
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

        let pct = if total_est > 0 {
            ((accumulated_inserted * 100) / total_est).clamp(0, 100) as u8
        } else {
            0
        };
        let message = if total_est > 0 {
            format!("Inserted {accumulated_inserted} of ~{total_est} rows")
        } else {
            format!("Inserted batch (+{inserted_count}) total={accumulated_inserted}")
        };
        sink.set_progress(pct, Some(message));

        // Logging de progreso (cada 5%) para tracing
        if total_est > 0 {
            let pct_i64 = pct as i64;
            if pct_i64 >= last_logged_pct + 5 {
                info!(
                    dataset_id = %meta.id,
                    symbol = %meta.symbol,
                    interval = %meta.interval,
                    inserted = accumulated_inserted,
                    total_est = total_est,
                    "{pct_i64}% approx ({accumulated_inserted}/{total_est})"
                );
                last_logged_pct = pct_i64;
            }
        } else {
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

    if sink.is_cancelled() {
        sink.append_log("ingestion canceled".to_string());
        return Ok(IngestOutcome::Canceled);
    }

    sink.set_progress(100, Some("Ingestion finished".to_string()));
    sink.append_log("ingestion finished".to_string());

    Ok(IngestOutcome::Completed { inserted_any })
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
