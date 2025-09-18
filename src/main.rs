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
    let port = config.port;
    let app = build_app(config)?; // -> axum 0.6: Router (con estado via .with_state(...))

    // en axum 0.6 se usa Server::bind(...).serve(app.into_make_service())
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(%addr, "starting exchange simulator server");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
