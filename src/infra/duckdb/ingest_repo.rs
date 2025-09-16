use std::fs;

use chrono::Utc;
use duckdb::params;
use uuid::Uuid;

use crate::{
    domain::{
        models::{DatasetFormat, DatasetMetadata},
        traits::MarketIngestor,
        value_objects::{DatasetPath, TimestampMs},
    },
    error::AppError,
};

use super::db::DuckDbPool;

pub struct DuckDbIngestRepo {
    pool: DuckDbPool,
}

impl DuckDbIngestRepo {
    pub fn new(pool: DuckDbPool) -> Self {
        Self { pool }
    }
}

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

#[async_trait::async_trait]
impl MarketIngestor for DuckDbIngestRepo {
    async fn register_dataset(
        &self,
        name: &str,
        path: DatasetPath,
        format: DatasetFormat,
    ) -> Result<DatasetMetadata, AppError> {
        if fs::metadata(path.as_str()).is_err() {
            return Err(AppError::Validation(format!(
                "dataset path does not exist: {}",
                path.as_str()
            )));
        }
        let pool = self.pool.clone();
        let name = name.to_string();
        let path_string = path.0.clone();
        pool.with_conn_async(move |conn| {
            let id = Uuid::new_v4();
            let created_at = TimestampMs::from(Utc::now().timestamp_millis());
            conn.execute(
                "INSERT INTO datasets(id, name, path, format, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, name, path_string, format.to_string(), created_at.0],
            )
            .map_err(|err| AppError::Database(format!("insert dataset failed: {err}")))?;
            Ok(DatasetMetadata {
                id,
                name,
                base_path: DatasetPath(path_string),
                format,
                created_at,
            })
        })
        .await
    }

    async fn list_datasets(&self) -> Result<Vec<DatasetMetadata>, AppError> {
        let pool = self.pool.clone();
        pool.with_conn_async(|conn| {
            let mut stmt = conn
                .prepare("SELECT id, name, path, format, created_at FROM datasets ORDER BY created_at DESC")
                .map_err(|err| AppError::Database(format!("prepare datasets failed: {err}")))?;
            let mut rows = stmt
                .query([])
                .map_err(|err| AppError::Database(format!("query datasets failed: {err}")))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|err| AppError::Database(format!("row iteration failed: {err}")))?
            {
                let format_str: String = row
                    .get(3)
                    .map_err(|err| AppError::Database(format!("column error: {err}")))?;
                let format = format_str
                    .parse::<DatasetFormat>()
                    .map_err(|err| AppError::Validation(err.to_string()))?;
                out.push(DatasetMetadata {
                    id: row
                        .get(0)
                        .map_err(|err| AppError::Database(format!("column error: {err}")))?,
                    name: row
                        .get(1)
                        .map_err(|err| AppError::Database(format!("column error: {err}")))?,
                    base_path: DatasetPath(row
                        .get::<_, String>(2)
                        .map_err(|err| AppError::Database(format!("column error: {err}")))?),
                    format,
                    created_at: TimestampMs(row
                        .get::<_, i64>(4)
                        .map_err(|err| AppError::Database(format!("column error: {err}")))?),
                });
            }
            Ok(out)
        })
        .await
    }

    async fn ingest_dataset(&self, dataset_id: Uuid) -> Result<(), AppError> {
        let pool = self.pool.clone();
        pool.with_conn_async(move |conn| {
            let mut stmt = conn
                .prepare("SELECT path, format FROM datasets WHERE id = ?1")
                .map_err(|err| AppError::Database(format!("prepare dataset lookup failed: {err}")))?;
            let mut rows = stmt
                .query(params![dataset_id])
                .map_err(|err| AppError::Database(format!("query dataset failed: {err}")))?;
            let row = rows
                .next()
                .map_err(|err| AppError::Database(format!("read dataset row failed: {err}")))?
                .ok_or_else(|| AppError::NotFound(format!("dataset {dataset_id} not found")))?;
            let path: String = row
                .get(0)
                .map_err(|err| AppError::Database(format!("column error: {err}")))?;
            let format_str: String = row
                .get(1)
                .map_err(|err| AppError::Database(format!("column error: {err}")))?;
            let format = format_str
                .parse::<DatasetFormat>()
                .map_err(|err| AppError::Validation(err.to_string()))?;

            match format {
                DatasetFormat::Csv => {
                    let import_sql = format!(
                        "INSERT INTO klines SELECT * FROM read_csv_auto('{}', HEADER TRUE)",
                        path
                    );
                    conn.execute(&import_sql, [])
                        .map_err(|err| AppError::Database(format!("csv ingest failed: {err}")))?;
                    let distinct_sql = format!(
                        "SELECT DISTINCT symbol FROM read_csv_auto('{}', HEADER TRUE)",
                        path
                    );
                    let mut symbols_stmt = conn
                        .prepare(&distinct_sql)
                        .map_err(|err| AppError::Database(format!("prepare distinct failed: {err}")))?;
                    let mut sym_rows = symbols_stmt
                        .query([])
                        .map_err(|err| AppError::Database(format!("query distinct failed: {err}")))?;
                    while let Some(sym_row) = sym_rows
                        .next()
                        .map_err(|err| AppError::Database(format!("symbol iteration failed: {err}")))?
                    {
                        let symbol: String = sym_row
                            .get(0)
                            .map_err(|err| AppError::Database(format!("symbol column failed: {err}")))?;
                        let (base, quote) = infer_base_quote(&symbol);
                        conn.execute(
                            "INSERT OR REPLACE INTO symbols(symbol, base, quote, active) VALUES (?1, ?2, ?3, TRUE)",
                            params![symbol, base, quote],
                        )
                        .map_err(|err| AppError::Database(format!("insert symbol failed: {err}")))?;
                    }
                }
                DatasetFormat::Parquet => {
                    let import_sql = format!(
                        "INSERT INTO klines SELECT * FROM read_parquet('{}')",
                        path
                    );
                    conn.execute(&import_sql, [])
                        .map_err(|err| AppError::Database(format!("parquet ingest failed: {err}")))?;
                    let distinct_sql = format!(
                        "SELECT DISTINCT symbol FROM read_parquet('{}')",
                        path
                    );
                    let mut symbols_stmt = conn
                        .prepare(&distinct_sql)
                        .map_err(|err| AppError::Database(format!("prepare distinct failed: {err}")))?;
                    let mut sym_rows = symbols_stmt
                        .query([])
                        .map_err(|err| AppError::Database(format!("query distinct failed: {err}")))?;
                    while let Some(sym_row) = sym_rows
                        .next()
                        .map_err(|err| AppError::Database(format!("symbol iteration failed: {err}")))?
                    {
                        let symbol: String = sym_row
                            .get(0)
                            .map_err(|err| AppError::Database(format!("symbol column failed: {err}")))?;
                        let (base, quote) = infer_base_quote(&symbol);
                        conn.execute(
                            "INSERT OR REPLACE INTO symbols(symbol, base, quote, active) VALUES (?1, ?2, ?3, TRUE)",
                            params![symbol, base, quote],
                        )
                        .map_err(|err| AppError::Database(format!("insert symbol failed: {err}")))?;
                    }
                }
            }
            Ok(())
        })
        .await
    }
}
