use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use std::str::FromStr;
use tracing::instrument;
use uuid::Uuid;

use crate::{
    api::errors::ApiResult,
    app::bootstrap::AppState,
    domain::value_objects::{Interval, Speed, TimestampMs},
    dto::sessions::{CreateSessionRequest, SessionResponse},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/sessions", post(create_session).get(list_sessions))
        .route("/api/v1/sessions/:id", get(get_session))
        .route("/api/v1/sessions/:id/start", post(start_session))
        .route("/api/v1/sessions/:id/pause", post(pause_session))
        .route("/api/v1/sessions/:id/resume", post(resume_session))
        .route("/api/v1/sessions/:id/seek", post(seek_session))
}

#[utoipa::path(
    post,
    path = "/api/v1/sessions",
    request_body = CreateSessionRequest,
    responses((status = 200, body = SessionResponse))
)]
#[instrument(skip(state, payload))]
pub async fn create_session(
    State(state): State<AppState>,
    Json(payload): Json<CreateSessionRequest>,
) -> ApiResult<Json<SessionResponse>> {
    let interval = Interval::from_str(&payload.interval)?;
    let speed = payload
        .speed
        .map(Speed::from)
        .unwrap_or(state.config.default_speed);
    let session = state
        .sessions_service
        .create_session(
            payload.symbols,
            interval,
            TimestampMs::from(payload.start_time),
            TimestampMs::from(payload.end_time),
            speed,
            payload.seed.unwrap_or(0),
        )
        .await?;
    Ok(Json(session.into()))
}

#[utoipa::path(get, path = "/api/v1/sessions", responses((status = 200, body = Vec<SessionResponse>)))]
#[instrument(skip(state))]
pub async fn list_sessions(State(state): State<AppState>) -> ApiResult<Json<Vec<SessionResponse>>> {
    let sessions = state.sessions_service.list_sessions().await?;
    Ok(Json(
        sessions.into_iter().map(SessionResponse::from).collect(),
    ))
}

#[utoipa::path(get, path = "/api/v1/sessions/{id}", params((name = "id", schema = Uuid)), responses((status = 200, body = SessionResponse)))]
#[instrument(skip(state))]
pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SessionResponse>> {
    let session = state.sessions_service.get_session(id).await?;
    Ok(Json(session.into()))
}

#[utoipa::path(post, path = "/api/v1/sessions/{id}/start", params((name = "id", schema = Uuid)), responses((status = 200, body = SessionResponse)))]
#[instrument(skip(state))]
pub async fn start_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SessionResponse>> {
    let session = state.sessions_service.start_session(id).await?;
    Ok(Json(session.into()))
}

#[utoipa::path(post, path = "/api/v1/sessions/{id}/pause", params((name = "id", schema = Uuid)), responses((status = 200, body = SessionResponse)))]
#[instrument(skip(state))]
pub async fn pause_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SessionResponse>> {
    let session = state.sessions_service.pause_session(id).await?;
    Ok(Json(session.into()))
}

#[utoipa::path(post, path = "/api/v1/sessions/{id}/resume", params((name = "id", schema = Uuid)), responses((status = 200, body = SessionResponse)))]
#[instrument(skip(state))]
pub async fn resume_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SessionResponse>> {
    let session = state.sessions_service.resume_session(id).await?;
    Ok(Json(session.into()))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeekQuery {
    to: i64,
}

#[utoipa::path(post, path = "/api/v1/sessions/{id}/seek", params((name = "id", schema = Uuid), (name = "to", schema = i64)), responses((status = 200, body = SessionResponse)))]
#[instrument(skip(state, query))]
pub async fn seek_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<SeekQuery>,
) -> ApiResult<Json<SessionResponse>> {
    let session = state
        .sessions_service
        .seek_session(id, TimestampMs::from(query.to))
        .await?;
    Ok(Json(session.into()))
}
