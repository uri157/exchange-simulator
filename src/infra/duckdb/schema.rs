// src/infra/duckdb/schema.rs
use duckdb::Connection;

use crate::error::AppError;

/// Aplica/asegura el esquema de DuckDB.
/// Ejecuta cada statement individualmente, ignorando líneas vacías y comentarios.
pub fn apply_schema(conn: &Connection) -> Result<(), AppError> {
    // Esquema consistente con la ingesta actual (Binance):
    // - symbols
    // - klines
    // - datasets (symbol/interval/start_time/end_time/source/status/created_at)
    // - índices útiles
    const SQL: &str = r#"
        -- Core tables
        CREATE TABLE IF NOT EXISTS symbols(
            symbol TEXT PRIMARY KEY,
            base   TEXT NOT NULL,
            quote  TEXT NOT NULL,
            active BOOLEAN NOT NULL
        );

        CREATE TABLE IF NOT EXISTS klines(
            symbol     TEXT   NOT NULL,
            interval   TEXT   NOT NULL,
            open_time  BIGINT NOT NULL,
            open       DOUBLE NOT NULL,
            high       DOUBLE NOT NULL,
            low        DOUBLE NOT NULL,
            close      DOUBLE NOT NULL,
            volume     DOUBLE NOT NULL,
            close_time BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS datasets(
            id         UUID   PRIMARY KEY,
            symbol     TEXT   NOT NULL,
            interval   TEXT   NOT NULL,
            start_time BIGINT NOT NULL,
            end_time   BIGINT NOT NULL,
            source     TEXT   NOT NULL, -- p.ej. 'binance'
            status     TEXT   NOT NULL, -- 'registered' | 'ingesting' | 'ready' | 'failed'
            created_at BIGINT NOT NULL
        );

        -- Idempotencia para velas
        CREATE UNIQUE INDEX IF NOT EXISTS ux_klines_symbol_interval_open
            ON klines(symbol, interval, open_time);

        -- Búsquedas típicas por datasets
        CREATE INDEX IF NOT EXISTS ix_datasets_symbol_interval
            ON datasets(symbol, interval);
    "#;

    // Ejecutar statement por statement evitando vacíos/comentarios
    for stmt in SQL.split(';') {
        let s = stmt.trim();
        if s.is_empty() {
            continue;
        }
        // ignorar líneas que son solo comentarios
        let only_comments = s
            .lines()
            .all(|l| l.trim().is_empty() || l.trim_start().starts_with("--"));
        if only_comments {
            continue;
        }

        conn.execute(s, [])
            .map_err(|e| AppError::Database(format!("schema apply failed: {e}")))?;
    }

    Ok(())
}
