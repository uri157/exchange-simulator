use std::{fs, path::Path, sync::Arc};

use duckdb::Connection;
use parking_lot::Mutex;

use crate::error::AppError;

// Importa el aplicador de esquema centralizado
use super::schema::apply_schema;

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

        // Aplica el esquema centralizado apenas se abre la conexión
        apply_schema(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
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
