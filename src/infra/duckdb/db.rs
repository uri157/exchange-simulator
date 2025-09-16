use std::{fs, path::Path, sync::Arc};

use duckdb::Connection;
use parking_lot::Mutex;

use crate::error::AppError;

#[derive(Clone)]
pub struct DuckDbPool {
    conn: Arc<Mutex<Connection>>,
}

impl DuckDbPool {
    pub fn new(path: &str) -> Result<Self, AppError> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)
                .map_err(|err| AppError::Internal(format!("failed to create data dir: {err}")))?;
        }
        let conn = Connection::open(path)
            .map_err(|err| AppError::Database(format!("failed to open duckdb: {err}")))?;
        let pool = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        pool.migrate()?;
        Ok(pool)
    }

    fn migrate(&self) -> Result<(), AppError> {
        self.with_conn(|conn| {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS symbols(
                    symbol TEXT PRIMARY KEY,
                    base TEXT NOT NULL,
                    quote TEXT NOT NULL,
                    active BOOLEAN NOT NULL
                );

                CREATE TABLE IF NOT EXISTS klines(
                    symbol TEXT NOT NULL,
                    interval TEXT NOT NULL,
                    open_time BIGINT NOT NULL,
                    open DOUBLE NOT NULL,
                    high DOUBLE NOT NULL,
                    low DOUBLE NOT NULL,
                    close DOUBLE NOT NULL,
                    volume DOUBLE NOT NULL,
                    close_time BIGINT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS datasets(
                    id UUID PRIMARY KEY,
                    name TEXT NOT NULL,
                    path TEXT NOT NULL,
                    format TEXT NOT NULL,
                    created_at BIGINT NOT NULL
                );
                "#,
            )
            .map_err(|err| AppError::Database(format!("migration error: {err}")))?;
            Ok(())
        })
    }

    pub fn connection(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    pub fn with_conn<F, R>(&self, f: F) -> Result<R, AppError>
    where
        F: FnOnce(&Connection) -> Result<R, AppError>,
    {
        let conn = self.conn.lock();
        f(&conn)
    }

    pub async fn with_conn_async<F, R>(&self, f: F) -> Result<R, AppError>
    where
        F: FnOnce(&Connection) -> Result<R, AppError> + Send + 'static,
        R: Send + 'static,
    {
        let pool = self.conn.clone();
        let result = tokio::task::spawn_blocking(move || {
            let conn = pool.lock();
            f(&conn)
        })
        .await
        .map_err(|err| AppError::Internal(format!("duckdb thread join error: {err}")))?;
        result
    }
}
