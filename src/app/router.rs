use axum::{routing::get, Json, Router};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;

use crate::{api, oas::ApiDoc};

/// Devuelve `Router<()>` (sin estado).
/// El estado se inyecta via `Extension` en `build_app`.
pub fn create_router() -> Router {
    let openapi = ApiDoc::openapi();

    Router::new()
        // Sirve el JSON de OpenAPI (sin Swagger UI)
        .route(
            "/api-docs/openapi.json",
            get({
                let openapi = openapi.clone();
                move || async { Json(openapi) }
            }),
        )
        .route("/ping", get(crate::api::errors::ping))
        .merge(api::v1::router()) // <-- Asegurate que este también devuelva Router (stateless)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
