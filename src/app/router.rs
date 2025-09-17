// src/app/router.rs
use axum::{routing::get, Router};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{api, app::bootstrap::AppState, oas::ApiDoc};

pub fn create_router(state: AppState) -> Router {
    let openapi = ApiDoc::openapi();

    Router::new()
        .route("/ping", get(api::errors::ping))
        .merge(api::v1::router())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
