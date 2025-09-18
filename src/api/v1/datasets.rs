use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use tracing::instrument;
use uuid::Uuid;

use crate::{
    api::errors::ApiResult,
    app::bootstrap::AppState,
    dto::datasets::{DatasetResponse, CreateDatasetRequest},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/datasets",
            post(register_dataset).get(list_datasets),
        )
        .route("/api/v1/datasets/{id}/ingest", post(ingest_dataset))
}

#[utoipa::path(
    post,
    path = "/api/v1/datasets",
    request_body = CreateDatasetRequest,
    responses((status = 200, body = DatasetResponse))
)]
#[instrument(skip(state, payload))]
pub async fn register_dataset(
    State(state): State<AppState>,
    Json(payload): Json<CreateDatasetRequest>,
) -> ApiResult<Json<DatasetResponse>> {
    let dataset = state
        .ingest_service
        .register_dataset(
            &payload.symbol,
            &payload.interval,
            payload.start_time,
            payload.end_time,
        )
        .await?;
    Ok(Json(dataset.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/datasets",
    responses((status = 200, body = Vec<DatasetResponse>))
)]
#[instrument(skip(state))]
pub async fn list_datasets(State(state): State<AppState>) -> ApiResult<Json<Vec<DatasetResponse>>> {
    let datasets = state.ingest_service.list_datasets().await?;
    Ok(Json(
        datasets.into_iter().map(DatasetResponse::from).collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/datasets/{id}/ingest",
    params(("id" = Uuid, Path, description = "Dataset ID")),
    responses((status = 204))
)]
#[instrument(skip(state))]
pub async fn ingest_dataset(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    state.ingest_service.ingest_dataset(id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
