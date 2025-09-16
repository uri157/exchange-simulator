use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use tracing::instrument;
use uuid::Uuid;

use crate::{
    api::errors::ApiResult,
    app::bootstrap::AppState,
    domain::value_objects::DatasetPath,
    dto::datasets::{DatasetResponse, RegisterDatasetRequest},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/datasets",
            post(register_dataset).get(list_datasets),
        )
        .route("/api/v1/datasets/:id/ingest", post(ingest_dataset))
}

#[utoipa::path(
    post,
    path = "/api/v1/datasets",
    request_body = RegisterDatasetRequest,
    responses((status = 200, body = DatasetResponse))
)]
#[instrument(skip(state, payload))]
pub async fn register_dataset(
    State(state): State<AppState>,
    Json(payload): Json<RegisterDatasetRequest>,
) -> ApiResult<Json<DatasetResponse>> {
    let dataset = state
        .ingest_service
        .register_dataset(
            &payload.name,
            DatasetPath::from(payload.path),
            payload.format,
        )
        .await?;
    Ok(Json(dataset.into()))
}

#[utoipa::path(get, path = "/api/v1/datasets", responses((status = 200, body = Vec<DatasetResponse>)))]
#[instrument(skip(state))]
pub async fn list_datasets(State(state): State<AppState>) -> ApiResult<Json<Vec<DatasetResponse>>> {
    let datasets = state.ingest_service.list_datasets().await?;
    Ok(Json(
        datasets.into_iter().map(DatasetResponse::from).collect(),
    ))
}

#[utoipa::path(post, path = "/api/v1/datasets/{id}/ingest", params((name = "id", schema = Uuid)), responses((status = 204)))]
#[instrument(skip(state))]
pub async fn ingest_dataset(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    state.ingest_service.ingest_dataset(id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
