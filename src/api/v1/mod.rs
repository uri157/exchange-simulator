use axum::Router;

use crate::app::bootstrap::AppState;

pub mod account;
pub mod datasets;
pub mod market;
pub mod market_binance;
pub mod orders;
pub mod sessions;
pub mod ws;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(market::router())
        .merge(market_binance::router())
        .merge(datasets::router())
        .merge(sessions::router())
        .merge(orders::router())
        .merge(account::router())
        .merge(ws::router())
}
