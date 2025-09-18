use std::sync::Arc;
use uuid::Uuid;

use crate::{
    domain::{models::DatasetMetadata, traits::MarketIngestor},
};
use super::ServiceResult;

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

    pub async fn ingest_dataset(&self, dataset_id: Uuid) -> ServiceResult<()> {
        self.ingestor.ingest_dataset(dataset_id).await
    }
}
