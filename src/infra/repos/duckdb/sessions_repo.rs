use std::str::FromStr;

use chrono::Utc;
use duckdb::{params, Row};
use uuid::Uuid;

use crate::{
    domain::{
        models::{SessionConfig, SessionStatus},
        traits::SessionsRepo,
        value_objects::{Interval, Speed, TimestampMs},
    },
    error::AppError,
    infra::duckdb::db::DuckDbPool,
};

#[derive(Clone)]
pub struct DuckDbSessionsRepo {
    pool: DuckDbPool,
}

impl DuckDbSessionsRepo {
    pub fn new(pool: DuckDbPool) -> Result<Self, AppError> {
        let repo = Self { pool };
        repo.ensure_table()?;
        Ok(repo)
    }

    fn ensure_table(&self) -> Result<(), AppError> {
        const SQL: &str = "
            CREATE TABLE IF NOT EXISTS sessions (
                id UUID PRIMARY KEY,
                symbols TEXT,
                interval TEXT,
                start_time BIGINT,
                end_time BIGINT,
                speed DOUBLE,
                status TEXT,
                seed BIGINT,
                created_at BIGINT,
                updated_at BIGINT
            );
        ";

        self.pool.with_conn(|conn| {
            conn.execute(SQL, []).map_err(|err| {
                AppError::Database(format!("create sessions table failed: {err}"))
            })?;
            Ok(())
        })?;

        Ok(())
    }

    fn status_to_str(status: &SessionStatus) -> &'static str {
        match status {
            SessionStatus::Created => "Created",
            SessionStatus::Running => "Running",
            SessionStatus::Paused => "Paused",
            SessionStatus::Ended => "Ended",
        }
    }

    fn status_from_str(status: &str) -> Result<SessionStatus, AppError> {
        match status {
            "Created" => Ok(SessionStatus::Created),
            "Running" => Ok(SessionStatus::Running),
            "Paused" => Ok(SessionStatus::Paused),
            "Ended" => Ok(SessionStatus::Ended),
            other => Err(AppError::Database(format!(
                "invalid session status: {other}"
            ))),
        }
    }

    fn row_to_session(row: &Row) -> Result<SessionConfig, AppError> {
        let id_str: String = row
            .get(0)
            .map_err(|err| AppError::Database(format!("session column error: {err}")))?;
        let session_id = Uuid::parse_str(&id_str)
            .map_err(|err| AppError::Database(format!("invalid session uuid: {err}")))?;

        let symbols_json: String = row
            .get(1)
            .map_err(|err| AppError::Database(format!("session column error: {err}")))?;
        let symbols: Vec<String> = serde_json::from_str(&symbols_json)
            .map_err(|err| AppError::Database(format!("invalid session symbols json: {err}")))?;

        let interval_str: String = row
            .get(2)
            .map_err(|err| AppError::Database(format!("session column error: {err}")))?;
        let interval = Interval::from_str(&interval_str)?;

        let start_time: i64 = row
            .get(3)
            .map_err(|err| AppError::Database(format!("session column error: {err}")))?;
        let end_time: i64 = row
            .get(4)
            .map_err(|err| AppError::Database(format!("session column error: {err}")))?;
        let speed_val: f64 = row
            .get(5)
            .map_err(|err| AppError::Database(format!("session column error: {err}")))?;
        let status_str: String = row
            .get(6)
            .map_err(|err| AppError::Database(format!("session column error: {err}")))?;
        let status = Self::status_from_str(&status_str)?;
        let seed_val: i64 = row
            .get(7)
            .map_err(|err| AppError::Database(format!("session column error: {err}")))?;
        let seed = u64::try_from(seed_val)
            .map_err(|err| AppError::Database(format!("invalid session seed: {err}")))?;
        let created_at: i64 = row
            .get(8)
            .map_err(|err| AppError::Database(format!("session column error: {err}")))?;
        let updated_at: i64 = row
            .get(9)
            .map_err(|err| AppError::Database(format!("session column error: {err}")))?;

        Ok(SessionConfig {
            session_id,
            symbols,
            interval,
            start_time: TimestampMs(start_time),
            end_time: TimestampMs(end_time),
            speed: Speed(speed_val),
            status,
            seed,
            created_at: TimestampMs(created_at),
            updated_at: TimestampMs(updated_at),
        })
    }

    async fn fetch_session(&self, session_id: Uuid) -> Result<SessionConfig, AppError> {
        let pool = self.pool.clone();
        let id = session_id;
        pool.with_conn_async(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, symbols, interval, start_time, end_time, speed, status, seed, created_at, updated_at \
                     FROM sessions WHERE id = ?1",
                )
                .map_err(|err| AppError::Database(format!("prepare session get failed: {err}")))?;
            let mut rows = stmt
                .query(params![id.to_string()])
                .map_err(|err| AppError::Database(format!("query session failed: {err}")))?;
            let row = rows
                .next()
                .map_err(|err| AppError::Database(format!("session row fetch failed: {err}")))?
                .ok_or_else(|| AppError::NotFound(format!("session {id} not found")))?;
            Self::row_to_session(&row)
        })
        .await
    }
}

#[async_trait::async_trait]
impl SessionsRepo for DuckDbSessionsRepo {
    async fn insert(&self, config: SessionConfig) -> Result<SessionConfig, AppError> {
        let pool = self.pool.clone();
        let to_insert = config.clone();
        pool.with_conn_async(move |conn| {
            let symbols = serde_json::to_string(&to_insert.symbols)
                .map_err(|err| AppError::Database(format!("serialize symbols failed: {err}")))?;
            let seed_val = i64::try_from(to_insert.seed)
                .map_err(|err| AppError::Database(format!("invalid session seed: {err}")))?;
            conn.execute(
                "INSERT INTO sessions (id, symbols, interval, start_time, end_time, speed, status, seed, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    to_insert.session_id.to_string(),
                    symbols,
                    to_insert.interval.as_str(),
                    to_insert.start_time.0,
                    to_insert.end_time.0,
                    to_insert.speed.0,
                    Self::status_to_str(&to_insert.status),
                    seed_val,
                    to_insert.created_at.0,
                    to_insert.updated_at.0
                ],
            )
            .map_err(|err| AppError::Database(format!("insert session failed: {err}")))?;
            Ok(to_insert)
        })
        .await
    }

    async fn update_status(
        &self,
        session_id: Uuid,
        status: SessionStatus,
    ) -> Result<SessionConfig, AppError> {
        let pool = self.pool.clone();
        let status_str = Self::status_to_str(&status).to_string();
        let now = TimestampMs::from(Utc::now().timestamp_millis());
        let id = session_id;
        pool.with_conn_async(move |conn| {
            let updated = conn
                .execute(
                    "UPDATE sessions SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    params![status_str, now.0, id.to_string()],
                )
                .map_err(|err| {
                    AppError::Database(format!("update session status failed: {err}"))
                })?;
            if updated == 0 {
                return Err(AppError::NotFound(format!("session {id} not found")));
            }
            Ok(())
        })
        .await?;

        self.fetch_session(session_id).await
    }

    async fn update_speed(
        &self,
        session_id: Uuid,
        speed: Speed,
    ) -> Result<SessionConfig, AppError> {
        speed.validate()?;
        let pool = self.pool.clone();
        let now = TimestampMs::from(Utc::now().timestamp_millis());
        let id = session_id;
        pool.with_conn_async(move |conn| {
            let updated = conn
                .execute(
                    "UPDATE sessions SET speed = ?1, updated_at = ?2 WHERE id = ?3",
                    params![speed.0, now.0, id.to_string()],
                )
                .map_err(|err| AppError::Database(format!("update session speed failed: {err}")))?;
            if updated == 0 {
                return Err(AppError::NotFound(format!("session {id} not found")));
            }
            Ok(())
        })
        .await?;

        self.fetch_session(session_id).await
    }

    async fn get(&self, session_id: Uuid) -> Result<SessionConfig, AppError> {
        self.fetch_session(session_id).await
    }

    async fn list(&self) -> Result<Vec<SessionConfig>, AppError> {
        let pool = self.pool.clone();
        pool.with_conn_async(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, symbols, interval, start_time, end_time, speed, status, seed, created_at, updated_at \
                     FROM sessions ORDER BY created_at DESC",
                )
                .map_err(|err| AppError::Database(format!("prepare session list failed: {err}")))?;
            let mut rows = stmt
                .query([])
                .map_err(|err| AppError::Database(format!("query sessions failed: {err}")))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|err| AppError::Database(format!("session row iteration failed: {err}")))?
            {
                out.push(Self::row_to_session(&row)?);
            }
            Ok(out)
        })
        .await
    }
}
