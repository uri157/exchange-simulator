use std::net::SocketAddr;

use tokio::net::TcpListener;
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
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).init();

    let config = AppConfig::from_env()?;
    let port = config.port;
    let app = build_app(config)?;

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "starting exchange simulator server");

    axum::serve(listener, app).await?;
    Ok(())
}
