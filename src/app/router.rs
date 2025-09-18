use axum::{routing::get, Json, Router};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;

use crate::{api, app::bootstrap::AppState, oas::ApiDoc};

pub fn create_router(state: AppState) -> Router<AppState> {
    let openapi = ApiDoc::openapi();

    Router::new()
        .route(
            "/api-docs/openapi.json",
            get({
                let openapi = openapi.clone();
                move || async { Json(openapi) }
            }),
        )
        .route("/ping", get(api::errors::ping))
        .merge(api::v1::router())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
