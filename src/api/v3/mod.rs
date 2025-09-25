use axum::Router;

pub mod ws;

pub fn router() -> Router {
    Router::new().merge(ws::router())
}
