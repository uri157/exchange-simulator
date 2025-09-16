use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    domain::{
        models::{Fill, Kline, Order, OrderSide, OrderStatus, OrderType, SessionStatus},
        traits::{OrderBookSim, OrdersRepo, SessionsRepo},
        value_objects::{Price, Quantity, TimestampMs},
    },
    error::AppError,
};

use super::{ServiceResult, account_service::AccountService, replay_service::ReplayService};

pub struct OrdersService {
    repo: Arc<dyn OrdersRepo>,
    sessions_repo: Arc<dyn SessionsRepo>,
    accounts: Arc<AccountService>,
    replay: Arc<ReplayService>,
    fills: Arc<RwLock<HashMap<Uuid, Vec<Fill>>>>,
}

impl OrdersService {
    pub fn new(
        repo: Arc<dyn OrdersRepo>,
        sessions_repo: Arc<dyn SessionsRepo>,
        accounts: Arc<AccountService>,
        replay: Arc<ReplayService>,
    ) -> Self {
        Self {
            repo,
            sessions_repo,
            accounts,
            replay,
            fills: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn validate_session(&self, session_id: Uuid, symbol: &str) -> ServiceResult<()> {
        let session = self.sessions_repo.get(session_id).await?;
        if !session.symbols.iter().any(|s| s == symbol) {
            return Err(AppError::Validation(format!(
                "symbol {symbol} is not part of session"
            )));
        }
        if matches!(session.status, SessionStatus::Ended) {
            return Err(AppError::Validation("session already ended".into()));
        }
        self.accounts.ensure_session_account(session_id).await?;
        Ok(())
    }

    async fn current_kline(&self, session_id: Uuid, symbol: &str) -> ServiceResult<Kline> {
        self.replay
            .latest_kline(session_id, symbol)
            .await?
            .ok_or_else(|| AppError::Validation("no market data for session yet".into()))
    }

    pub async fn place_order(
        &self,
        session_id: Uuid,
        symbol: String,
        side: OrderSide,
        order_type: OrderType,
        quantity: Quantity,
        price: Option<Price>,
        client_order_id: Option<String>,
    ) -> ServiceResult<(Order, Vec<Fill>)> {
        self.validate_session(session_id, &symbol).await?;
        if quantity.0 <= 0.0 {
            return Err(AppError::Validation("quantity must be positive".into()));
        }
        let order_id = Uuid::new_v4();
        let now = TimestampMs::from(Utc::now().timestamp_millis());
        let mut order = Order {
            order_id,
            session_id,
            client_order_id,
            symbol: symbol.clone(),
            side,
            order_type,
            price,
            quantity,
            filled_quantity: Quantity::default(),
            status: OrderStatus::New,
            created_at: now,
        };
        let mut fills = Vec::new();
        let latest = self.current_kline(session_id, &symbol).await?;
        let execution_price = match order.order_type {
            OrderType::Market => Some(latest.close),
            OrderType::Limit => {
                let limit_price = order
                    .price
                    .ok_or_else(|| AppError::Validation("limit order requires price".into()))?;
                if should_fill_limit(side, limit_price, &latest) {
                    Some(limit_price)
                } else {
                    None
                }
            }
        };

        if let Some(exec_price) = execution_price {
            let fill = Fill {
                order_id,
                symbol: symbol.clone(),
                price: exec_price,
                quantity,
                fee: Price(0.0),
                trade_time: latest.close_time,
            };
            fills.push(fill.clone());
            order.status = OrderStatus::Filled;
            order.filled_quantity = quantity;
            self.accounts
                .apply_fill(session_id, &symbol, side, exec_price, quantity)
                .await?;
            let mut guard = self.fills.write().await;
            guard.entry(session_id).or_default().push(fill);
        }

        self.repo.upsert(order.clone()).await?;
        Ok((order, fills))
    }

    pub async fn cancel_order(&self, session_id: Uuid, order_id: Uuid) -> ServiceResult<Order> {
        let mut order = self.repo.get(session_id, order_id).await?;
        if matches!(order.status, OrderStatus::Filled | OrderStatus::Canceled) {
            return Err(AppError::Validation("order cannot be canceled".into()));
        }
        order.status = OrderStatus::Canceled;
        self.repo.upsert(order.clone()).await?;
        Ok(order)
    }

    pub async fn get_order(&self, session_id: Uuid, order_id: Uuid) -> ServiceResult<Order> {
        self.repo.get(session_id, order_id).await
    }

    pub async fn get_by_client_id(
        &self,
        session_id: Uuid,
        client_id: &str,
    ) -> ServiceResult<Order> {
        self.repo.get_by_client_id(session_id, client_id).await
    }

    pub async fn list_open(
        &self,
        session_id: Uuid,
        symbol: Option<&str>,
    ) -> ServiceResult<Vec<Order>> {
        self.repo.list_open(session_id, symbol).await
    }

    pub async fn my_trades(&self, session_id: Uuid, symbol: &str) -> ServiceResult<Vec<Fill>> {
        let guard = self.fills.read().await;
        let trades = guard
            .get(&session_id)
            .map(|fills| {
                fills
                    .iter()
                    .filter(|f| f.symbol == symbol)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        Ok(trades)
    }
}

#[async_trait::async_trait]
impl OrderBookSim for OrdersService {
    async fn new_order(
        &self,
        session_id: Uuid,
        order: Order,
    ) -> Result<(Order, Vec<Fill>), AppError> {
        self.place_order(
            session_id,
            order.symbol.clone(),
            order.side,
            order.order_type,
            order.quantity,
            order.price,
            order.client_order_id.clone(),
        )
        .await
    }

    async fn cancel_order(&self, session_id: Uuid, order_id: Uuid) -> Result<Order, AppError> {
        self.cancel_order(session_id, order_id).await
    }
}

fn should_fill_limit(side: OrderSide, limit_price: Price, kline: &Kline) -> bool {
    match side {
        OrderSide::Buy => limit_price.0 >= kline.low.0,
        OrderSide::Sell => limit_price.0 <= kline.high.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::Kline;
    use crate::domain::value_objects::{Interval, Price, Quantity, TimestampMs};

    fn sample_kline() -> Kline {
        Kline {
            symbol: "BTCUSDT".to_string(),
            interval: Interval::new("1m"),
            open_time: TimestampMs(0),
            open: Price(100.0),
            high: Price(110.0),
            low: Price(95.0),
            close: Price(105.0),
            volume: Quantity(1.0),
            close_time: TimestampMs(60_000),
        }
    }

    #[test]
    fn limit_buy_executes_when_price_below_low() {
        let kline = sample_kline();
        assert!(super::should_fill_limit(
            OrderSide::Buy,
            Price(96.0),
            &kline
        ));
        assert!(!super::should_fill_limit(
            OrderSide::Buy,
            Price(90.0),
            &kline
        ));
    }

    #[test]
    fn limit_sell_executes_when_price_above_high() {
        let kline = sample_kline();
        assert!(super::should_fill_limit(
            OrderSide::Sell,
            Price(109.0),
            &kline
        ));
        assert!(!super::should_fill_limit(
            OrderSide::Sell,
            Price(120.0),
            &kline
        ));
    }
}
