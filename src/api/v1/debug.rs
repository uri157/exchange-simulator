use axum::{routing::get, Extension, Json, Router};
use duckdb::Row;
use serde::Serialize;
use tracing::instrument;

use crate::{api::errors::ApiResult, app::bootstrap::AppState, error::AppError};

#[derive(Serialize)]
pub struct DebugDbResponse {
    /// Ruta canónica que DuckDB reporta para la DB principal
    pub main_db_path: String,
    /// Salida de PRAGMA database_list
    pub database_list: Vec<DatabaseInfo>,
    /// Conteos básicos de tablas
    pub counts: TableCounts,
}

#[derive(Serialize)]
pub struct DatabaseInfo {
    pub seq: i64,
    pub name: String,
    pub file: String,
}

#[derive(Serialize)]
pub struct TableCounts {
    pub datasets: i64,
    pub klines: i64,
    pub symbols: i64,
}

pub fn router() -> Router {
    Router::new().route("/api/v1/debug/db", get(debug_db))
}

#[utoipa::path(
    get,
    path = "/api/v1/debug/db",
    responses(
        (status = 200, description = "DB debug info", body = DebugDbResponse)
    )
)]
#[instrument(skip(state))]
pub async fn debug_db(Extension(state): Extension<AppState>) -> ApiResult<Json<DebugDbResponse>> {
    // Usamos el pool directo para consultar PRAGMA y conteos
    let pool = state.duck_pool.clone();

    let (database_list, counts) = pool
        .with_conn_async(|conn| {
            // PRAGMA database_list
            let mut stmt = conn
                .prepare("PRAGMA database_list")
                .map_err(|e| AppError::Database(format!("prepare database_list: {e}")))?;
            let mut rows = stmt
                .query([])
                .map_err(|e| AppError::Database(format!("query database_list: {e}")))?;

            let mut dbs: Vec<DatabaseInfo> = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| AppError::Database(format!("iter database_list: {e}")))?
            {
                dbs.push(parse_database_row(&row)?);
            }

            // Conteos
            let datasets = scalar_count(conn, "datasets")?;
            let klines = scalar_count(conn, "klines")?;
            let symbols = scalar_count(conn, "symbols")?;

            Ok::<_, AppError>((
                dbs,
                TableCounts {
                    datasets,
                    klines,
                    symbols,
                },
            ))
        })
        .await?;

    // Tomamos el path del entry "main" si existe; si no, devolvemos el primero o vacío
    let main_db_path = database_list
        .iter()
        .find(|d| d.name == "main")
        .map(|d| d.file.clone())
        .or_else(|| database_list.get(0).map(|d| d.file.clone()))
        .unwrap_or_default();

    Ok(Json(DebugDbResponse {
        main_db_path,
        database_list,
        counts,
    }))
}

fn parse_database_row(row: &Row) -> Result<DatabaseInfo, AppError> {
    // DuckDB expone: seq (BIGINT), name (VARCHAR), file (VARCHAR)
    let seq: i64 = row
        .get(0)
        .map_err(|e| AppError::Database(format!("db_list.seq: {e}")))?;
    let name: String = row
        .get(1)
        .map_err(|e| AppError::Database(format!("db_list.name: {e}")))?;
    let file: String = row
        .get(2)
        .map_err(|e| AppError::Database(format!("db_list.file: {e}")))?;
    Ok(DatabaseInfo { seq, name, file })
}

fn scalar_count(conn: &duckdb::Connection, table: &str) -> Result<i64, AppError> {
    let sql = format!("SELECT COUNT(*) FROM {}", table);
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Database(format!("prepare count {}: {e}", table)))?;
    let mut rows = stmt
        .query([])
        .map_err(|e| AppError::Database(format!("query count {}: {e}", table)))?;
    let row = rows
        .next()
        .map_err(|e| AppError::Database(format!("row count {}: {e}", table)))?
        .ok_or_else(|| AppError::Database(format!("no row for count {}", table)))?;
    let n: i64 = row
        .get(0)
        .map_err(|e| AppError::Database(format!("get count {}: {e}", table)))?;
    Ok(n)
}
