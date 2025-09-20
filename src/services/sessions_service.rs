use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        models::{SessionConfig, SessionStatus},
        traits::{Clock, ReplayEngine, SessionsRepo},
        value_objects::{Interval, Speed, TimestampMs},
    },
    error::AppError,
    infra::ws::broadcaster::SessionBroadcaster,
};

use super::ServiceResult;

pub struct SessionsService {
    sessions_repo: Arc<dyn SessionsRepo>,
    clock: Arc<dyn Clock>,
    replay: Arc<dyn ReplayEngine>,
    broadcaster: SessionBroadcaster,
}

impl SessionsService {
    pub fn new(
        sessions_repo: Arc<dyn SessionsRepo>,
        clock: Arc<dyn Clock>,
        replay: Arc<dyn ReplayEngine>,
        broadcaster: SessionBroadcaster,
    ) -> Self {
        Self {
            sessions_repo,
            clock,
            replay,
            broadcaster,
        }
    }

    pub async fn create_session(
        &self,
        symbols: Vec<String>,
        interval: Interval,
        start_time: TimestampMs,
        end_time: TimestampMs,
        speed: Speed,
        seed: u64,
    ) -> ServiceResult<SessionConfig> {
        speed.validate()?;

        if symbols.is_empty() {
            return Err(crate::error::AppError::Validation(
                "at least one symbol is required".into(),
            ));
        }
        if end_time.0 <= start_time.0 {
            return Err(crate::error::AppError::Validation(
                "end_time must be greater than start_time".into(),
            ));
        }

        let session_id = Uuid::new_v4();
        let now = TimestampMs::from(Utc::now().timestamp_millis());

        let config = SessionConfig {
            session_id,
            symbols,
            interval,
            start_time,
            end_time,
            speed,
            enabled: true,
            status: SessionStatus::Created,
            seed,
            created_at: now,
            updated_at: now,
        };

        // Persist first
        let inserted = self.sessions_repo.insert(config.clone()).await?;

        // Ensure the clock has a slot for this session and position it at start_time
        // (init is a no-op for clocks that don't need pre-initialization).
        let _ = self.clock.init_session(session_id, start_time).await;
        self.clock.advance_to(session_id, start_time).await?;

        Ok(inserted)
    }

    pub async fn start_session(&self, session_id: Uuid) -> ServiceResult<SessionConfig> {
        let session = self.sessions_repo.get(session_id).await?;

        ensure_enabled(&session)?;

        match session.status {
            SessionStatus::Running => {
                return Err(crate::error::AppError::Conflict(
                    "session is already running".into(),
                ))
            }
            SessionStatus::Created | SessionStatus::Paused | SessionStatus::Ended => {}
        }

        self.clock
            .init_session(session_id, session.start_time)
            .await?;
        self.clock.set_speed(session_id, session.speed).await?;

        let previous_status = session.status.clone();
        let running_session = self
            .sessions_repo
            .update_status(session_id, SessionStatus::Running)
            .await?;

        if let Err(err) = self.replay.start(running_session.clone()).await {
            let _ = self
                .sessions_repo
                .update_status(session_id, previous_status)
                .await;
            return Err(err);
        }

        self.clock.resume(session_id).await?;

        Ok(running_session)
    }

    pub async fn pause_session(&self, session_id: Uuid) -> ServiceResult<SessionConfig> {
        let session = self.sessions_repo.get(session_id).await?;
        ensure_enabled(&session)?;

        self.clock.pause(session_id).await?;
        self.replay.pause(session_id).await?;
        self.sessions_repo
            .update_status(session_id, SessionStatus::Paused)
            .await
    }

    pub async fn resume_session(&self, session_id: Uuid) -> ServiceResult<SessionConfig> {
        let session = self.sessions_repo.get(session_id).await?;
        ensure_enabled(&session)?;

        self.clock.resume(session_id).await?;
        self.replay.resume(session_id).await?;
        self.sessions_repo
            .update_status(session_id, SessionStatus::Running)
            .await
    }

    pub async fn seek_session(
        &self,
        session_id: Uuid,
        to: TimestampMs,
    ) -> ServiceResult<SessionConfig> {
        let session = self.sessions_repo.get(session_id).await?;

        ensure_enabled(&session)?;

        if to.0 < session.start_time.0 || to.0 > session.end_time.0 {
            return Err(crate::error::AppError::Validation(
                "seek target outside session range".into(),
            ));
        }

        let current = self.clock.now(session_id).await?;
        if session.status == SessionStatus::Running && to.0 < current.0 {
            return Err(crate::error::AppError::Validation(
                "cannot seek backwards while session is running".into(),
            ));
        }

        self.clock.advance_to(session_id, to).await?;
        self.replay.seek(session_id, to).await?;
        self.sessions_repo.get(session_id).await
    }

    pub async fn list_sessions(&self) -> ServiceResult<Vec<SessionConfig>> {
        self.sessions_repo.list().await
    }

    pub async fn get_session(&self, session_id: Uuid) -> ServiceResult<SessionConfig> {
        self.sessions_repo.get(session_id).await
    }

    pub async fn enable_session(&self, session_id: Uuid) -> ServiceResult<SessionConfig> {
        let session = self.sessions_repo.get(session_id).await?;
        if session.enabled {
            return Ok(session);
        }

        self.sessions_repo.set_enabled(session_id, true).await?;
        self.sessions_repo.get(session_id).await
    }

    pub async fn disable_session(&self, session_id: Uuid) -> ServiceResult<SessionConfig> {
        let session = self.sessions_repo.get(session_id).await?;

        self.replay.stop(session_id).await?;

        if matches!(session.status, SessionStatus::Running) {
            let _ = self
                .sessions_repo
                .update_status(session_id, SessionStatus::Paused)
                .await?;
        }

        self.sessions_repo.set_enabled(session_id, false).await?;
        self.broadcaster.close(session_id).await;

        self.sessions_repo.get(session_id).await
    }

    pub async fn delete_session(&self, session_id: Uuid) -> ServiceResult<()> {
        // Ensure it exists (and fetch current status for potential cleanup)
        let session = self.sessions_repo.get(session_id).await?;

        self.replay.stop(session_id).await?;

        if matches!(session.status, SessionStatus::Running) {
            let _ = self
                .sessions_repo
                .update_status(session_id, SessionStatus::Paused)
                .await?;
        }

        self.sessions_repo.delete(session_id).await?;
        self.broadcaster.close(session_id).await;

        Ok(())
    }
}

fn ensure_enabled(session: &SessionConfig) -> ServiceResult<()> {
    if !session.enabled {
        return Err(AppError::Conflict("session is disabled".into()));
    }
    Ok(())
}
