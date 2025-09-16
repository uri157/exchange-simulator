use std::sync::Arc;
use uuid::Uuid;

use crate::domain::{
    models::{DatasetFormat, DatasetMetadata},
    traits::MarketIngestor,
    value_objects::DatasetPath,
};

use super::ServiceResult;

pub struct IngestService {
    ingestor: Arc<dyn MarketIngestor>,
}

impl IngestService {
    pub fn new(ingestor: Arc<dyn MarketIngestor>) -> Self {
        Self { ingestor }
    }

    pub async fn register_dataset(
        &self,
        name: &str,
        path: DatasetPath,
        format: DatasetFormat,
    ) -> ServiceResult<DatasetMetadata> {
        self.ingestor.register_dataset(name, path, format).await
    }

    pub async fn list_datasets(&self) -> ServiceResult<Vec<DatasetMetadata>> {
        self.ingestor.list_datasets().await
    }

    pub async fn ingest_dataset(&self, dataset_id: Uuid) -> ServiceResult<()> {
        self.ingestor.ingest_dataset(dataset_id).await
    }
}
