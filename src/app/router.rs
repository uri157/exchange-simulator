use axum::{routing::get, Json, Router};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::{openapi::OpenApi as OpenApiDoc, OpenApi};

use crate::{api, oas::ApiDoc};

pub fn create_router() -> Router {
    let openapi = ApiDoc::openapi();
    let v3_doc: OpenApiDoc = crate::oas::BinanceV3Api::openapi();

    let app = Router::new()
        .route(
            "/api-docs/openapi.json",
            get({
                let openapi = openapi.clone();
                move || async { Json(openapi) }
            }),
        )
        .route("/ping", get(crate::api::errors::ping))
        .merge(api::v1::router())
        .merge(api::v3::router());

    let v3_json = Router::new().route(
        "/api-docs/openapi.v3.json",
        get({
            let v3_doc = v3_doc.clone();
            move || async { Json(v3_doc) }
        }),
    );

    app.merge(v3_json)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
