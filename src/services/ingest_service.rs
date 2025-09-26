use std::sync::Arc;
use uuid::Uuid;

use super::ServiceResult;
use crate::domain::{
    models::{dataset_status::DatasetStatus, DatasetMetadata},
    traits::MarketIngestor,
};

#[derive(Clone)]
pub struct IngestService {
    ingestor: Arc<dyn MarketIngestor>,
}

impl IngestService {
    pub fn new(ingestor: Arc<dyn MarketIngestor>) -> Self {
        Self { ingestor }
    }

    pub async fn register_dataset(
        &self,
        symbol: &str,
        interval: &str,
        start_time: i64,
        end_time: i64,
    ) -> ServiceResult<DatasetMetadata> {
        self.ingestor
            .register_dataset(symbol, interval, start_time, end_time)
            .await
    }

    pub async fn list_datasets(&self) -> ServiceResult<Vec<DatasetMetadata>> {
        self.ingestor.list_datasets().await
    }

    pub async fn get_dataset(&self, dataset_id: Uuid) -> ServiceResult<DatasetMetadata> {
        self.ingestor.get_dataset(dataset_id).await
    }

    pub async fn ingest_dataset(&self, dataset_id: Uuid) -> ServiceResult<()> {
        self.ingestor.ingest_dataset(dataset_id).await
    }

    pub async fn delete_dataset(&self, dataset_id: Uuid) -> ServiceResult<()> {
        self.ingestor.delete_dataset(dataset_id).await
    }

    pub async fn update_dataset_status(
        &self,
        dataset_id: Uuid,
        status: DatasetStatus,
    ) -> ServiceResult<()> {
        self.ingestor
            .update_dataset_status(dataset_id, status)
            .await
    }

    pub async fn list_ready_symbols(&self) -> ServiceResult<Vec<String>> {
        self.ingestor.list_ready_symbols().await
    }

    pub async fn list_ready_intervals(&self, symbol: &str) -> ServiceResult<Vec<String>> {
        self.ingestor.list_ready_intervals(symbol).await
    }

    pub async fn get_range(&self, symbol: &str, interval: &str) -> ServiceResult<(i64, i64)> {
        self.ingestor.get_range(symbol, interval).await
    }
}
