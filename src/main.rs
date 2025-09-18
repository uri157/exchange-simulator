use std::net::SocketAddr;
use tracing_subscriber::{fmt, EnvFilter};

use crate::{app::bootstrap::build_app, infra::config::AppConfig};

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
    let app = build_app(config)?; // Router<AppState> (axum 0.6)

    // axum 0.6 + hyper 0.14
    tracing::info!(%addr, "starting exchange simulator server");
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;

    Ok(())
}
