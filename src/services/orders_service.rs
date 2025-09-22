use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        models::{Liquidity, Order, OrderSide, OrderStatus, OrderType, SessionStatus},
        traits::{Clock, OrdersRepo, SessionsRepo},
        value_objects::{Price, Quantity, TimestampMs},
    },
    error::AppError,
};

use super::{account_service::AccountService, replay_service::ReplayService, ServiceResult};

pub struct OrdersService {
    repo: Arc<dyn OrdersRepo>,
    sessions_repo: Arc<dyn SessionsRepo>,
    accounts: Arc<AccountService>,
    replay: Arc<ReplayService>,
    clock: Arc<dyn Clock>,
}

impl OrdersService {
    pub fn new(
        repo: Arc<dyn OrdersRepo>,
        sessions_repo: Arc<dyn SessionsRepo>,
        accounts: Arc<AccountService>,
        replay: Arc<ReplayService>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            sessions_repo,
            accounts,
            replay,
            clock,
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

    async fn now(&self, session_id: Uuid) -> TimestampMs {
        self.clock
            .now(session_id)
            .await
            .unwrap_or_else(|_| TimestampMs::from(Utc::now().timestamp_millis()))
    }

    fn ensure_positive(quantity: Quantity) -> ServiceResult<()> {
        if quantity.0 <= 0.0 {
            return Err(AppError::Validation("quantity must be positive".into()));
        }
        Ok(())
    }

    pub async fn place_market(
        &self,
        session_id: Uuid,
        symbol: String,
        side: OrderSide,
        quantity: Quantity,
        client_order_id: Option<String>,
    ) -> ServiceResult<Order> {
        Self::ensure_positive(quantity)?;
        self.validate_session(session_id, &symbol).await?;

        let now = self.now(session_id).await;
        let order = Order {
            id: Uuid::new_v4(),
            session_id,
            client_order_id,
            symbol,
            side,
            order_type: OrderType::Market,
            price: None,
            quantity,
            filled_quantity: Quantity::default(),
            status: OrderStatus::New,
            created_at: now,
            updated_at: now,
            maker_taker: Some(Liquidity::Taker),
        };

        Ok(self.repo.create(order).await?)
    }

    pub async fn place_limit(
        &self,
        session_id: Uuid,
        symbol: String,
        side: OrderSide,
        price: Price,
        quantity: Quantity,
        client_order_id: Option<String>,
    ) -> ServiceResult<Order> {
        if price.0 <= 0.0 {
            return Err(AppError::Validation("price must be positive".into()));
        }
        Self::ensure_positive(quantity)?;
        self.validate_session(session_id, &symbol).await?;

        let latest_trade = self
            .replay
            .latest_trade(session_id, &symbol)
            .await?
            .map(|trade| trade.price);

        let maker_taker = latest_trade.and_then(|last_price| {
            let crossed = match side {
                OrderSide::Buy => last_price <= price.0,
                OrderSide::Sell => last_price >= price.0,
            };
            if crossed {
                Some(Liquidity::Taker)
            } else {
                None
            }
        });

        let now = self.now(session_id).await;
        let order = Order {
            id: Uuid::new_v4(),
            session_id,
            client_order_id,
            symbol,
            side,
            order_type: OrderType::Limit,
            price: Some(price),
            quantity,
            filled_quantity: Quantity::default(),
            status: OrderStatus::New,
            created_at: now,
            updated_at: now,
            maker_taker,
        };

        Ok(self.repo.create(order).await?)
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
    ) -> ServiceResult<Order> {
        match order_type {
            OrderType::Market => {
                self.place_market(session_id, symbol, side, quantity, client_order_id)
                    .await
            }
            OrderType::Limit => {
                let price = price
                    .ok_or_else(|| AppError::Validation("limit order requires price".into()))?;
                self.place_limit(session_id, symbol, side, price, quantity, client_order_id)
                    .await
            }
        }
    }

    pub async fn cancel_order(&self, session_id: Uuid, order_id: Uuid) -> ServiceResult<Order> {
        let order = self.repo.get(session_id, order_id).await?;
        if matches!(
            order.status,
            OrderStatus::Filled | OrderStatus::Canceled | OrderStatus::Expired
        ) {
            return Err(AppError::Validation("order cannot be canceled".into()));
        }
        Ok(self.repo.cancel(session_id, order_id).await?)
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

    pub async fn my_trades(
        &self,
        session_id: Uuid,
        symbol: &str,
    ) -> ServiceResult<Vec<crate::domain::models::Fill>> {
        self.repo.list_fills(session_id, Some(symbol)).await
    }

    pub async fn order_fills(
        &self,
        session_id: Uuid,
        order_id: Uuid,
    ) -> ServiceResult<Vec<crate::domain::models::Fill>> {
        self.repo.list_order_fills(session_id, order_id).await
    }
}

#[async_trait::async_trait]
impl crate::domain::traits::OrderBookSim for OrdersService {}
