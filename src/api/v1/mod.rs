use axum::Router;

pub mod datasets;
pub mod debug;
pub mod market;
pub mod market_binance;
pub mod sessions;
pub mod ws;

pub fn router() -> Router {
    Router::new()
        .merge(market::router())
        .merge(market_binance::router())
        .merge(datasets::router())
        .merge(sessions::router())
        .merge(ws::router())
        .merge(debug::router())
}
