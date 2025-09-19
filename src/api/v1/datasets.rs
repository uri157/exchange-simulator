use axum::{
    extract::Path,
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use tracing::{error, info, instrument};
use uuid::Uuid;

use crate::{
    api::errors::ApiResult,
    app::bootstrap::AppState,
    dto::datasets::{
        CreateDatasetRequest, DatasetResponse, IntervalOnly, RangeResponse, SymbolOnly,
    },
};

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/v1/datasets",
            post(register_dataset).get(list_datasets),
        )
        .route("/api/v1/datasets/:id/ingest", post(ingest_dataset))
        // Nuevos endpoints para consultar lo disponible en la BD (datasets/klines)
        .route("/api/v1/datasets/symbols", get(get_ready_symbols))
        .route(
            "/api/v1/datasets/:symbol/intervals",
            get(get_ready_intervals),
        )
        .route(
            "/api/v1/datasets/:symbol/:interval/range",
            get(get_symbol_interval_range),
        )
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
    let ingest = state.ingest_service.clone();
    tokio::spawn(async move {
        info!(%id, "starting dataset ingestion task");
        if let Err(err) = ingest.ingest_dataset(id).await {
            error!(%id, error = %err, "dataset ingestion failed");
        } else {
            info!(%id, "dataset ingestion finished");
        }
    });
    Ok(StatusCode::ACCEPTED)
}

#[utoipa::path(
    get,
    path = "/api/v1/datasets/symbols",
    responses((status = 200, body = Vec<SymbolOnly>))
)]
#[instrument(skip(state))]
pub async fn get_ready_symbols(
    Extension(state): Extension<AppState>,
) -> ApiResult<Json<Vec<SymbolOnly>>> {
    let symbols = state.ingest_service.list_ready_symbols().await?;
    Ok(Json(symbols.into_iter().map(SymbolOnly::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/datasets/{symbol}/intervals",
    params(("symbol" = String, Path, description = "Trading pair")),
    responses((status = 200, body = Vec<IntervalOnly>))
)]
#[instrument(skip(state), fields(symbol = %symbol))]
pub async fn get_ready_intervals(
    Extension(state): Extension<AppState>,
    Path(symbol): Path<String>,
) -> ApiResult<Json<Vec<IntervalOnly>>> {
    let intervals = state.ingest_service.list_ready_intervals(&symbol).await?;
    Ok(Json(
        intervals.into_iter().map(IntervalOnly::from).collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/datasets/{symbol}/{interval}/range",
    params(
        ("symbol" = String, Path, description = "Trading pair"),
        ("interval" = String, Path, description = "Interval (e.g., 1m, 1h)")
    ),
    responses((status = 200, body = RangeResponse))
)]
#[instrument(skip(state), fields(symbol = %path.0, interval = %path.1))]
pub async fn get_symbol_interval_range(
    Extension(state): Extension<AppState>,
    Path(path): Path<(String, String)>,
) -> ApiResult<Json<RangeResponse>> {
    let (symbol, interval) = path;
    let range = state.ingest_service.get_range(&symbol, &interval).await?;
    Ok(Json(RangeResponse::from(range)))
}

// Manual tests:
// curl -s localhost:3001/api/v1/datasets/symbols
// curl -s localhost:3001/api/v1/datasets/BTCUSDT/intervals
// curl -s localhost:3001/api/v1/datasets/BTCUSDT/1m/range
