use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Extension, Json, Router,
};
use futures_core::{future::Future, stream::Stream};
use serde::Deserialize;
use serde_json;
use std::{
    collections::VecDeque,
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::broadcast;
use tracing::{error, info, instrument};
use uuid::Uuid;

use crate::{
    api::errors::ApiResult,
    app::bootstrap::AppState,
    domain::models::{dataset_status::DatasetStatus, DatasetMetadata},
    dto::datasets::{
        CreateDatasetRequest, DatasetDetail, DatasetLogEntry, DatasetResponse, IntervalOnly,
        RangeResponse, SymbolOnly,
    },
    error::AppError,
    infra::progress::ingestion_registry::{DatasetProgressEvent, ProgressSnapshot},
};

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/v1/datasets",
            post(register_dataset).get(list_datasets),
        )
        .route(
            "/api/v1/datasets/:id",
            get(get_dataset_detail).delete(delete_dataset_handler),
        )
        .route("/api/v1/datasets/:id/ingest", post(ingest_dataset))
        .route("/api/v1/datasets/:id/cancel", post(cancel_dataset))
        .route("/api/v1/datasets/:id/events", get(dataset_events))
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
    let progress = state.ingestion_progress.clone();
    let responses = datasets
        .into_iter()
        .map(|meta| {
            let snapshot = progress.snapshot_or_default(meta.id, meta.status, meta.updated_at);
            build_dataset_response(meta, &snapshot)
        })
        .collect();
    Ok(Json(responses))
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

#[derive(Debug, Deserialize)]
pub struct DeleteDatasetQuery {
    force: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/api/v1/datasets/{id}",
    params(("id" = Uuid, Path, description = "Dataset ID")),
    responses((status = 200, body = DatasetDetail))
)]
#[instrument(skip(state), fields(dataset_id = %id))]
pub async fn get_dataset_detail(
    Extension(state): Extension<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<DatasetDetail>> {
    let meta = state.ingest_service.get_dataset(id).await?;
    let snapshot =
        state
            .ingestion_progress
            .snapshot_or_default(meta.id, meta.status, meta.updated_at);
    Ok(Json(build_dataset_detail(meta, &snapshot)))
}

#[utoipa::path(
    get,
    path = "/api/v1/datasets/{id}/events",
    params(("id" = Uuid, Path, description = "Dataset ID")),
    responses((status = 200, description = "Dataset progress event stream"))
)]
#[instrument(skip(state), fields(dataset_id = %id))]
pub async fn dataset_events(
    Extension(state): Extension<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let meta = state.ingest_service.get_dataset(id).await?;
    let (snapshot, receiver) =
        state
            .ingestion_progress
            .subscribe(meta.id, meta.status, meta.updated_at);

    let mut initial_events = VecDeque::new();
    for evt in initial_progress_events(&snapshot) {
        initial_events.push_back(Ok(event_to_sse(evt)));
    }

    let stream = DatasetEventStream::new(initial_events, receiver);

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))))
}

struct DatasetEventStream {
    initial: VecDeque<Result<Event, Infallible>>,
    receiver: broadcast::Receiver<DatasetProgressEvent>,
}

impl DatasetEventStream {
    fn new(
        initial: VecDeque<Result<Event, Infallible>>,
        receiver: broadcast::Receiver<DatasetProgressEvent>,
    ) -> Self {
        Self { initial, receiver }
    }
}

impl Stream for DatasetEventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(evt) = self.initial.pop_front() {
            return Poll::Ready(Some(evt));
        }

        loop {
            let recv_fut = self.receiver.recv();
            tokio::pin!(recv_fut);

            match recv_fut.poll(cx) {
                Poll::Ready(Ok(evt)) => return Poll::Ready(Some(Ok(event_to_sse(evt)))),
                Poll::Ready(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Poll::Ready(Err(broadcast::error::RecvError::Closed)) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/datasets/{id}",
    params(
        ("id" = Uuid, Path, description = "Dataset ID"),
        (
            "force" = bool,
            Query,
            description = "Cancel ingestion if running before deletion"
        )
    ),
    responses((status = 204))
)]
#[instrument(skip(state, params), fields(dataset_id = %id))]
pub async fn delete_dataset_handler(
    Extension(state): Extension<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<DeleteDatasetQuery>,
) -> ApiResult<StatusCode> {
    let dataset = state.ingest_service.get_dataset(id).await?;
    if dataset.status == DatasetStatus::Ingesting && !params.force.unwrap_or(false) {
        return Err(AppError::Conflict(
            "dataset ingestion in progress".to_string(),
        ));
    }

    if dataset.status == DatasetStatus::Ingesting {
        state.ingestion_progress.append_log(
            id,
            "force delete requested; canceling ingestion".to_string(),
        );
        state.ingestion_progress.cancel(id);
        state.ingestion_progress.set_status(
            id,
            DatasetStatus::Canceled,
            Some("Ingestion canceled by delete".to_string()),
        );
        state
            .ingest_service
            .update_dataset_status(id, DatasetStatus::Canceled)
            .await?;
    }

    state.ingest_service.delete_dataset(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/datasets/{id}/cancel",
    params(("id" = Uuid, Path, description = "Dataset ID")),
    responses((status = 202))
)]
#[instrument(skip(state), fields(dataset_id = %id))]
pub async fn cancel_dataset(
    Extension(state): Extension<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let dataset = state.ingest_service.get_dataset(id).await?;
    if dataset.status != DatasetStatus::Ingesting {
        return Err(AppError::Conflict("dataset is not ingesting".to_string()));
    }

    state
        .ingestion_progress
        .append_log(id, "cancel requested".to_string());
    state.ingestion_progress.cancel(id);
    state.ingestion_progress.set_status(
        id,
        DatasetStatus::Canceled,
        Some("Ingestion canceled".to_string()),
    );
    state
        .ingest_service
        .update_dataset_status(id, DatasetStatus::Canceled)
        .await?;
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

fn build_dataset_response(meta: DatasetMetadata, snapshot: &ProgressSnapshot) -> DatasetResponse {
    DatasetResponse {
        id: meta.id,
        symbol: meta.symbol,
        interval: meta.interval,
        start_time: meta.start_time,
        end_time: meta.end_time,
        status: snapshot.status,
        progress: snapshot.progress,
        last_message: snapshot.last_message.clone(),
        created_at: meta.created_at,
        updated_at: snapshot.updated_at,
    }
}

fn build_dataset_detail(meta: DatasetMetadata, snapshot: &ProgressSnapshot) -> DatasetDetail {
    let logs = snapshot
        .logs
        .iter()
        .cloned()
        .map(|log| DatasetLogEntry {
            line: log.line,
            ts: log.ts,
        })
        .collect();
    DatasetDetail {
        id: meta.id,
        symbol: meta.symbol,
        interval: meta.interval,
        start_time: meta.start_time,
        end_time: meta.end_time,
        status: snapshot.status,
        progress: snapshot.progress,
        last_message: snapshot.last_message.clone(),
        created_at: meta.created_at,
        updated_at: snapshot.updated_at,
        logs,
    }
}

fn initial_progress_events(snapshot: &ProgressSnapshot) -> Vec<DatasetProgressEvent> {
    let mut events = Vec::new();
    events.push(DatasetProgressEvent::Status {
        status: snapshot.status,
        updated_at: snapshot.updated_at,
    });
    events.push(DatasetProgressEvent::Progress {
        progress: snapshot.progress,
        last_message: snapshot.last_message.clone(),
        updated_at: snapshot.updated_at,
    });
    for log in &snapshot.logs {
        events.push(DatasetProgressEvent::Log {
            line: log.line.clone(),
            ts: log.ts,
        });
    }
    match snapshot.status {
        DatasetStatus::Ready | DatasetStatus::Canceled => {
            events.push(DatasetProgressEvent::Done {
                status: snapshot.status,
                updated_at: snapshot.updated_at,
            });
        }
        DatasetStatus::Failed => {
            events.push(DatasetProgressEvent::Error {
                status: snapshot.status,
                last_message: snapshot.last_message.clone(),
                updated_at: snapshot.updated_at,
            });
        }
        _ => {}
    }
    events
}

fn event_to_sse(event: DatasetProgressEvent) -> Event {
    let data = serde_json::to_string(&event).expect("serialize dataset progress event");
    Event::default().event(event.event_name()).data(data)
}
