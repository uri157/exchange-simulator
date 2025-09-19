use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{models::DatasetMetadata, traits::MarketIngestor},
    error::AppError,
};

use super::{
    db::DuckDbPool,
    ingest_runner, // orquestador de descarga+ingesta
    ingest_sql,    // helpers de SQL (insert/select/update/list)
};

#[derive(Clone)]
pub struct DuckDbIngestRepo {
    pool: DuckDbPool,
}

impl DuckDbIngestRepo {
    pub fn new(pool: DuckDbPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl MarketIngestor for DuckDbIngestRepo {
    async fn register_dataset(
        &self,
        symbol: &str,
        interval: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<DatasetMetadata, AppError> {
        if start_time <= 0 || end_time <= 0 || end_time <= start_time {
            return Err(AppError::Validation(
                "invalid time range for dataset".to_string(),
            ));
        }

        let meta = DatasetMetadata {
            id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            interval: interval.to_string(),
            start_time,
            end_time,
            status: "registered".to_string(),
            created_at: Utc::now().timestamp_millis(),
        };

        let meta_for_insert = meta.clone();
        let pool = self.pool.clone();
        pool.with_conn_async(move |conn| {
            ingest_sql::insert_dataset_row(conn, &meta_for_insert)?;
            Ok::<_, AppError>(())
        })
        .await?;

        Ok(meta)
    }

    async fn list_datasets(&self) -> Result<Vec<DatasetMetadata>, AppError> {
        let pool = self.pool.clone();
        pool.with_conn_async(|conn| ingest_sql::list_datasets_query(conn))
            .await
    }

    async fn ingest_dataset(&self, dataset_id: Uuid) -> Result<(), AppError> {
        // 1) Cargar metadata
        let (symbol, interval, start_time, end_time) = {
            let pool = self.pool.clone();
            pool.with_conn_async({
                let dataset_id = dataset_id;
                move |conn| ingest_sql::select_dataset_meta(conn, dataset_id)
            })
            .await?
        };

        let meta = DatasetMetadata {
            id: dataset_id,
            symbol,
            interval,
            start_time,
            end_time,
            status: "ingesting".to_string(),
            created_at: 0, // no se usa aquí
        };

        // 2) Marcar status = ingesting
        {
            let pool = self.pool.clone();
            pool.with_conn_async({
                let id = dataset_id;
                move |conn| ingest_sql::mark_dataset_status(conn, id, "ingesting")
            })
            .await?;
        }

        // 3) Orquestar descarga + ingesta
        let inserted_any = ingest_runner::run_ingest(&self.pool, &meta).await?;

        // 4) Marcar status final
        let final_status = if inserted_any { "ready" } else { "failed" };
        let pool = self.pool.clone();
        pool.with_conn_async({
            let id = dataset_id;
            let status = final_status.to_string();
            move |conn| ingest_sql::mark_dataset_status(conn, id, &status)
        })
        .await?;

        Ok(())
    }

    async fn list_ready_symbols(&self) -> Result<Vec<String>, AppError> {
        let pool = self.pool.clone();
        pool.with_conn_async(|conn| ingest_sql::list_ready_dataset_symbols(conn))
            .await
    }

    async fn list_ready_intervals(&self, symbol: &str) -> Result<Vec<String>, AppError> {
        let pool = self.pool.clone();
        let symbol_owned = symbol.to_string();
        pool.with_conn_async(move |conn| {
            ingest_sql::list_ready_intervals_for_symbol(conn, &symbol_owned)
        })
        .await
    }

    async fn get_range(&self, symbol: &str, interval: &str) -> Result<(i64, i64), AppError> {
        let pool = self.pool.clone();
        let sym_owned = symbol.to_string();
        let intv_owned = interval.to_string();
        let maybe_range = pool
            .with_conn_async(move |conn| {
                ingest_sql::get_range_for_symbol_interval(conn, &sym_owned, &intv_owned)
            })
            .await?;

        maybe_range.ok_or_else(|| {
            AppError::NotFound(format!(
                "range for symbol {symbol} interval {interval} not found"
            ))
        })
    }
}
