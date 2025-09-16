use std::sync::Arc;

use crate::{
    domain::{
        models::Kline,
        traits::MarketStore,
        value_objects::{Interval, TimestampMs},
    },
    error::AppError,
};

use super::ServiceResult;

pub struct MarketService {
    store: Arc<dyn MarketStore>,
}

impl MarketService {
    pub fn new(store: Arc<dyn MarketStore>) -> Self {
        Self { store }
    }

    pub async fn exchange_info(&self) -> Result<Vec<crate::domain::models::Symbol>, AppError> {
        self.store.list_symbols().await
    }

    pub async fn klines(
        &self,
        symbol: &str,
        interval: Interval,
        start: Option<TimestampMs>,
        end: Option<TimestampMs>,
        limit: Option<usize>,
    ) -> ServiceResult<Vec<Kline>> {
        self.store
            .get_klines(symbol, &interval, start, end, limit)
            .await
    }
}
