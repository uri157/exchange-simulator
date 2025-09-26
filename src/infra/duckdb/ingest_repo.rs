use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        models::{dataset_status::DatasetStatus, DatasetMetadata},
        traits::MarketIngestor,
    },
    error::AppError,
    infra::ingest::progress_sink::ProgressSink,
    infra::progress::ingestion_registry::IngestionProgressRegistry,
};

use super::{
    db::DuckDbPool,
    ingest_runner, // orquestador de descarga+ingesta
    ingest_sql,    // helpers de SQL (insert/select/update/list)
};

#[derive(Clone)]
pub struct DuckDbIngestRepo {
    pool: DuckDbPool,
    progress: IngestionProgressRegistry,
}

impl DuckDbIngestRepo {
    pub fn new(pool: DuckDbPool, progress: IngestionProgressRegistry) -> Self {
        Self { pool, progress }
    }

    async fn persist_status(
        &self,
        dataset_id: Uuid,
        status: DatasetStatus,
    ) -> Result<(), AppError> {
        let pool = self.pool.clone();
        let status_str = status.as_storage_str().to_string();
        pool.with_conn_async(move |conn| {
            ingest_sql::mark_dataset_status(conn, dataset_id, &status_str)
        })
        .await
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

        let now = Utc::now().timestamp_millis();
        let meta = DatasetMetadata {
            id: Uuid::new_v4(),
            symbol: symbol.to_string(),
            interval: interval.to_string(),
            start_time,
            end_time,
            status: DatasetStatus::Registered,
            progress: 0,
            last_message: None,
            created_at: now,
            updated_at: now,
        };

        let meta_for_insert = meta.clone();
        let pool = self.pool.clone();
        pool.with_conn_async(move |conn| {
            ingest_sql::insert_dataset_row(conn, &meta_for_insert)?;
            Ok::<_, AppError>(())
        })
        .await?;

        self.progress
            .bootstrap(meta.id, DatasetStatus::Registered, now, None);

        Ok(meta)
    }

    async fn list_datasets(&self) -> Result<Vec<DatasetMetadata>, AppError> {
        let pool = self.pool.clone();
        pool.with_conn_async(|conn| ingest_sql::list_datasets_query(conn))
            .await
    }

    async fn get_dataset(&self, dataset_id: Uuid) -> Result<DatasetMetadata, AppError> {
        let pool = self.pool.clone();
        pool.with_conn_async(move |conn| ingest_sql::get_dataset(conn, dataset_id))
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
            status: DatasetStatus::Ingesting,
            progress: 0,
            last_message: None,
            created_at: 0, // no se usa aquí
            updated_at: 0,
        };

        let progress_handle = self
            .progress
            .start_ingest(dataset_id, DatasetStatus::Registered);
        progress_handle.set_status(
            DatasetStatus::Ingesting,
            Some("Ingestion started".to_string()),
        );

        // 2) Marcar status = ingesting
        self.persist_status(dataset_id, DatasetStatus::Ingesting)
            .await?;

        // 3) Orquestar descarga + ingesta
        match ingest_runner::run_ingest(&self.pool, &meta, &progress_handle).await {
            Ok(ingest_runner::IngestOutcome::Completed { inserted_any }) => {
                let final_status = if inserted_any {
                    DatasetStatus::Ready
                } else {
                    DatasetStatus::Failed
                };
                let message = if inserted_any {
                    Some("Dataset ready".to_string())
                } else {
                    progress_handle
                        .append_log("ingestion finished without inserting rows".to_string());
                    Some("No rows ingested".to_string())
                };
                self.persist_status(dataset_id, final_status).await?;
                progress_handle.set_status(final_status, message);
                Ok(())
            }
            Ok(ingest_runner::IngestOutcome::Canceled) => {
                self.persist_status(dataset_id, DatasetStatus::Canceled)
                    .await?;
                progress_handle.set_status(
                    DatasetStatus::Canceled,
                    Some("Ingestion canceled".to_string()),
                );
                Ok(())
            }
            Err(err) => {
                let msg = err.to_string();
                self.persist_status(dataset_id, DatasetStatus::Failed)
                    .await?;
                progress_handle.append_log(format!("ingestion failed: {msg}"));
                progress_handle.set_status(DatasetStatus::Failed, Some(msg));
                Err(err)
            }
        }
    }

    async fn list_ready_symbols(&self) -> Result<Vec<String>, AppError> {
        let pool = self.pool.clone();
        pool.with_conn_async(|conn| ingest_sql::list_ready_dataset_symbols(conn))
            .await
    }

    async fn delete_dataset(&self, dataset_id: Uuid) -> Result<(), AppError> {
        let pool = self.pool.clone();
        pool.with_conn_async(move |conn| ingest_sql::delete_dataset(conn, dataset_id))
            .await?;
        self.progress.clear(dataset_id);
        Ok(())
    }

    async fn update_dataset_status(
        &self,
        dataset_id: Uuid,
        status: DatasetStatus,
    ) -> Result<(), AppError> {
        self.persist_status(dataset_id, status).await
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
