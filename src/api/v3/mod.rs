use axum::Router;

pub mod account;
pub mod orders;
pub mod ws;

pub fn router() -> Router {
    Router::new()
        .merge(ws::router())
        .merge(orders::router())
        .merge(account::router())
}
