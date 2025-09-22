use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
};

use dotenvy::dotenv;

use crate::{
    domain::{
        models::{FeeConfig, MarketMode},
        value_objects::Speed,
    },
    error::AppError,
};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub port: u16,
    pub duckdb_path: String, // siempre absoluto
    pub data_dir: String,    // puede ser relativo, solo informativo
    pub default_speed: Speed,
    pub default_market_mode: MarketMode,
    pub ws_buffer: usize,
    pub max_session_clients: usize,
    pub fees: FeeConfig,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, AppError> {
        dotenv().ok();

        let port: u16 = env::var("PORT")
            .unwrap_or_else(|_| "3001".to_string())
            .parse()
            .map_err(|err| AppError::Validation(format!("invalid PORT: {err}")))?;

        // Valor crudo desde env (puede ser relativo)
        let duckdb_path_raw =
            env::var("DUCKDB_PATH").unwrap_or_else(|_| "./data/market.duckdb".to_string());

        // Normalizamos a ABSOLUTO sin requerir que exista el archivo todavía.
        let duckdb_path = to_absolute_path(&duckdb_path_raw);

        let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string());

        let default_speed: f64 = env::var("DEFAULT_SPEED")
            .unwrap_or_else(|_| "1.0".to_string())
            .parse()
            .map_err(|err| AppError::Validation(format!("invalid DEFAULT_SPEED: {err}")))?;

        let default_market_mode =
            env::var("DEFAULT_MARKET_MODE").unwrap_or_else(|_| "kline".to_string());
        let default_market_mode = MarketMode::from_str(&default_market_mode)?;

        let ws_buffer: usize = env::var("WS_BUFFER")
            .unwrap_or_else(|_| "1024".to_string())
            .parse()
            .map_err(|err| AppError::Validation(format!("invalid WS_BUFFER: {err}")))?;

        let max_session_clients: usize = env::var("MAX_SESSION_CLIENTS")
            .unwrap_or_else(|_| "100".to_string())
            .parse()
            .map_err(|err| AppError::Validation(format!("invalid MAX_SESSION_CLIENTS: {err}")))?;

        let maker_bps: u32 = env::var("FEES_MAKER_BPS")
            .unwrap_or_else(|_| "8".to_string())
            .parse()
            .map_err(|err| AppError::Validation(format!("invalid FEES_MAKER_BPS: {err}")))?;
        let taker_bps: u32 = env::var("FEES_TAKER_BPS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .map_err(|err| AppError::Validation(format!("invalid FEES_TAKER_BPS: {err}")))?;
        let partial_fills: bool = env::var("FEES_PARTIAL_FILLS")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .map_err(|err| AppError::Validation(format!("invalid FEES_PARTIAL_FILLS: {err}")))?;

        Ok(Self {
            port,
            duckdb_path,
            data_dir,
            default_speed: Speed::from(default_speed),
            default_market_mode,
            ws_buffer,
            max_session_clients,
            fees: FeeConfig {
                maker_bps,
                taker_bps,
                partial_fills,
            },
        })
    }
}

/// Convierte un path (posiblemente relativo) a un path absoluto, sin fallar si el archivo no existe.
fn to_absolute_path(input: &str) -> String {
    let p = Path::new(input);

    if p.is_absolute() {
        return p.to_string_lossy().into_owned();
    }

    // Expansión simple de "~/" usando HOME (y fallback a USERPROFILE en Windows).
    if let Some(rest) = input.strip_prefix("~/") {
        if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
            return Path::new(&home).join(rest).to_string_lossy().into_owned();
        }
    }

    // Relativo al cwd.
    let cwd: PathBuf = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    cwd.join(p).to_string_lossy().into_owned()
}
