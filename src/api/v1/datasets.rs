use axum::{
    extract::Path,
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use tracing::{instrument, info, error};
use uuid::Uuid;

use crate::{
    api::errors::ApiResult,
    app::bootstrap::AppState,
    dto::datasets::{CreateDatasetRequest, DatasetResponse},
};

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/v1/datasets",
            post(register_dataset).get(list_datasets),
        )
        // Axum usa :id para path params
        .route("/api/v1/datasets/:id/ingest", post(ingest_dataset))
}

#[utoipa::path(
    post,
    path = "/api/v1/datasets",
    request_body = CreateDatasetRequest,
    responses((status = 200, body = DatasetResponse))
)]
#[instrument(skip(state, payload))]
pub async fn register_dataset(
    Extension(state): Extension<AppState>,
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
pub async fn list_datasets(
    Extension(state): Extension<AppState>,
) -> ApiResult<Json<Vec<DatasetResponse>>> {
    let datasets = state.ingest_service.list_datasets().await?;
    Ok(Json(
        datasets.into_iter().map(DatasetResponse::from).collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/datasets/{id}/ingest",
    params(("id" = Uuid, Path, description = "Dataset ID")),
    responses((status = 202))
)]
#[instrument(skip(state), fields(dataset_id = %id))]
pub async fn ingest_dataset(
    Extension(state): Extension<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    // Disparamos la ingesta en background para evitar timeout del request.
    let ingest = state.ingest_service.clone();
    tokio::spawn(async move {
        info!(%id, "starting dataset ingestion task");
        if let Err(err) = ingest.ingest_dataset(id).await {
            error!(%id, error = %err, "dataset ingestion failed");
        } else {
            info!(%id, "dataset ingestion finished");
        }
    });

    // Respondemos inmediatamente.
    Ok(StatusCode::ACCEPTED)
}
