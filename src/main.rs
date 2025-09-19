use std::net::SocketAddr;
use tracing_subscriber::{fmt, EnvFilter};

use crate::{
    api::v1::ws::WS_ROUTE,
    app::bootstrap::build_app,
    infra::{config::AppConfig, ws::broadcaster::WsBroadcastMessage},
};

mod api;
mod app;
mod domain;
mod dto;
mod error;
mod infra;
mod oas;
mod services;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // logging
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).init();

    // config y router
    let config = AppConfig::from_env()?;
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let app = build_app(config.clone())?; // -> Router<()> con Extension(AppState) ya aplicada

    let ws_example = WsBroadcastMessage::example_json();
    tracing::info!("WS_EXPOSED_PATH={}", WS_ROUTE);
    tracing::info!("WS_EXPECTED_QUERY=sessionId=<uuid>&streams=kline@{interval}:{symbol}[, ...]");
    tracing::info!("WS_SAMPLE_MESSAGE={}", ws_example);
    tracing::info!("PREGUNTAS_ABIERTAS=Confirmar reintentos del cliente tras cierre NORMAL");

    // axum 0.6 + hyper 0.14
    tracing::info!(%addr, "starting exchange simulator server");
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;

    Ok(())
}
