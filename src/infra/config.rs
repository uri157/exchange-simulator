use std::env;

use dotenvy::dotenv;

use crate::{domain::value_objects::Speed, error::AppError};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub port: u16,
    pub duckdb_path: String,
    pub data_dir: String,
    pub default_speed: Speed,
    pub ws_buffer: usize,
    pub max_session_clients: usize,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, AppError> {
        dotenv().ok();
        let port = env::var("PORT").unwrap_or_else(|_| "3001".to_string());
        let duckdb_path =
            env::var("DUCKDB_PATH").unwrap_or_else(|_| "./data/market.duckdb".to_string());
        let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string());
        let default_speed: f64 = env::var("DEFAULT_SPEED")
            .unwrap_or_else(|_| "1.0".to_string())
            .parse()
            .map_err(|err| AppError::Validation(format!("invalid DEFAULT_SPEED: {err}")))?;
        let ws_buffer: usize = env::var("WS_BUFFER")
            .unwrap_or_else(|_| "1024".to_string())
            .parse()
            .map_err(|err| AppError::Validation(format!("invalid WS_BUFFER: {err}")))?;
        let max_session_clients: usize = env::var("MAX_SESSION_CLIENTS")
            .unwrap_or_else(|_| "100".to_string())
            .parse()
            .map_err(|err| AppError::Validation(format!("invalid MAX_SESSION_CLIENTS: {err}")))?;

        Ok(Self {
            port: port
                .parse()
                .map_err(|err| AppError::Validation(format!("invalid PORT: {err}")))?,
            duckdb_path,
            data_dir,
            default_speed: Speed::from(default_speed),
            ws_buffer,
            max_session_clients,
        })
    }
}
