// src/infra/clock.rs
use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    domain::value_objects::{Speed, TimestampMs},
    error::AppError,
};

#[derive(Clone, Debug)]
struct ClockState {
    current_time: TimestampMs,
    speed: Speed,
    paused: bool,
}

#[derive(Clone)]
pub struct SimulatedClock {
    inner: Arc<RwLock<HashMap<Uuid, ClockState>>>,
    default_speed: Speed,
}

impl SimulatedClock {
    pub fn new(default_speed: Speed) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            default_speed,
        }
    }

    pub async fn init_session(&self, session_id: Uuid, start_time: TimestampMs) -> Result<(), AppError> {
        let mut guard = self.inner.write().await;
        guard.entry(session_id).or_insert(ClockState {
            current_time: start_time,
            speed: self.default_speed,
            paused: true,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn clock_can_advance_and_pause() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let clock = SimulatedClock::new(Speed(2.0));
            let session_id = Uuid::new_v4();
            clock.init_session(session_id, TimestampMs(0)).await.unwrap();
            clock.resume(session_id).await.unwrap();
            clock
                .advance_to(session_id, TimestampMs(500))
                .await
                .unwrap();
            assert_eq!(clock.now(session_id).await.unwrap().0, 500);
            assert_eq!(clock.current_speed(session_id).await.unwrap().0, 2.0);
            clock.pause(session_id).await.unwrap();
            assert!(clock.is_paused(session_id).await.unwrap());
        });
    }
}

#[async_trait]
impl crate::domain::traits::Clock for SimulatedClock {
    async fn init_session(&self, session_id: Uuid, start_time: TimestampMs) -> Result<(), AppError> {
        let mut guard = self.inner.write().await;
        guard.entry(session_id).or_insert(ClockState {
            current_time: start_time,
            speed: self.default_speed,
            paused: true,
        });
        Ok(())
    }

    async fn now(&self, session_id: Uuid) -> Result<TimestampMs, AppError> {
        let guard = self.inner.read().await;
        guard
            .get(&session_id)
            .map(|state| state.current_time)
            .ok_or_else(|| AppError::NotFound(format!("clock for session {session_id} not found")))
    }

    async fn set_speed(&self, session_id: Uuid, speed: Speed) -> Result<(), AppError> {
        speed.validate()?;
        let mut guard = self.inner.write().await;
        let state = guard.get_mut(&session_id).ok_or_else(|| {
            AppError::NotFound(format!("clock for session {session_id} not found"))
        })?;
        state.speed = speed;
        Ok(())
    }

    async fn advance_to(&self, session_id: Uuid, to: TimestampMs) -> Result<(), AppError> {
        let mut guard = self.inner.write().await;
        let state = guard.get_mut(&session_id).ok_or_else(|| {
            AppError::NotFound(format!("clock for session {session_id} not found"))
        })?;
        if to.0 < state.current_time.0 {
            if state.paused {
                state.current_time = to;
                return Ok(());
            }
            return Err(AppError::Validation("cannot move clock backwards".into()));
        }
        state.current_time = to;
        Ok(())
    }

    async fn pause(&self, session_id: Uuid) -> Result<(), AppError> {
        let mut guard = self.inner.write().await;
        let state = guard.get_mut(&session_id).ok_or_else(|| {
            AppError::NotFound(format!("clock for session {session_id} not found"))
        })?;
        state.paused = true;
        Ok(())
    }

    async fn resume(&self, session_id: Uuid) -> Result<(), AppError> {
        let mut guard = self.inner.write().await;
        let state = guard.get_mut(&session_id).ok_or_else(|| {
            AppError::NotFound(format!("clock for session {session_id} not found"))
        })?;
        state.paused = false;
        Ok(())
    }

    async fn is_paused(&self, session_id: Uuid) -> Result<bool, AppError> {
        let guard = self.inner.read().await;
        guard
            .get(&session_id)
            .map(|state| state.paused)
            .ok_or_else(|| AppError::NotFound(format!("clock for session {session_id} not found")))
    }

    async fn current_speed(&self, session_id: Uuid) -> Result<Speed, AppError> {
        let guard = self.inner.read().await;
        guard
            .get(&session_id)
            .map(|state| state.speed)
            .ok_or_else(|| AppError::NotFound(format!("clock for session {session_id} not found")))
    }
}
