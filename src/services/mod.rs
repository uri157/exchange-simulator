use crate::error::AppError;

pub type ServiceResult<T> = Result<T, AppError>;

pub mod account_service;
pub mod ingest_service;
pub mod market_service;
pub mod orders_service;
pub mod replay_service;
pub mod sessions_service;

// Re-export para imports simples: `use crate::services::IngestService;`
pub use ingest_service::IngestService;
