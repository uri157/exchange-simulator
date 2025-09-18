use axum::{routing::get, Router};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{api, app::bootstrap::AppState, oas::ApiDoc};

pub fn create_router(state: AppState) -> Router<AppState> {
    let openapi = ApiDoc::openapi();

    // Importante: NO tipar swagger como `Router` para no fijar el state en `()`.
    // Aplicamos `with_state(state)` **al final** para que todo el árbol pase a `Router<AppState>`.
    Router::new()
        .route("/ping", get(api::errors::ping))
        .merge(api::v1::router())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
