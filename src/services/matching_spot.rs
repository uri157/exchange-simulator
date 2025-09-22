use std::sync::Arc;

use tracing::warn;
use uuid::Uuid;

use crate::domain::{
    models::{AggTrade, FeeConfig, Fill, Liquidity, Order, OrderSide, OrderStatus, OrderType},
    traits::OrdersRepo,
    value_objects::{Price, Quantity, TimestampMs},
};

use super::{
    account_service::{split_symbol, AccountService},
    ServiceResult,
};

pub struct SpotMatcher {
    orders_repo: Arc<dyn OrdersRepo>,
    accounts: Arc<AccountService>,
    fee_config: FeeConfig,
}

impl SpotMatcher {
    pub fn new(
        orders_repo: Arc<dyn OrdersRepo>,
        accounts: Arc<AccountService>,
        fee_config: FeeConfig,
    ) -> Self {
        Self {
            orders_repo,
            accounts,
            fee_config,
        }
    }

    pub async fn on_trade(&self, session_id: Uuid, trade: &AggTrade) {
        if let Err(err) = self.process_trade(session_id, trade).await {
            warn!(%session_id, trade_id = trade.trade_id, error = %err, "spot matcher failed");
        }
    }

    async fn process_trade(&self, session_id: Uuid, trade: &AggTrade) -> ServiceResult<()> {
        let orders = self.orders_repo.list_active(session_id).await?;
        for mut order in orders
            .into_iter()
            .filter(|order| order.symbol == trade.symbol)
        {
            if self.orders_repo.has_fill(order.id, trade.trade_id).await? {
                continue;
            }
            if !self.should_fill(&order, trade) {
                continue;
            }

            let remaining = (order.quantity.0 - order.filled_quantity.0).max(0.0);
            if remaining <= f64::EPSILON {
                continue;
            }

            let fill_qty = self.fill_quantity(remaining, trade.qty);
            if fill_qty <= 0.0 {
                continue;
            }

            let maker = self.is_maker(&order);
            let fee_rate = if maker {
                self.fee_config.maker_bps as f64 / 10_000.0
            } else {
                self.fee_config.taker_bps as f64 / 10_000.0
            };

            let fill = self
                .execute_fill(&mut order, trade, fill_qty, maker, fee_rate)
                .await?;

            self.orders_repo.append_fill(fill.clone()).await?;
            self.orders_repo.update(order).await?;
        }
        Ok(())
    }

    async fn execute_fill(
        &self,
        order: &mut Order,
        trade: &AggTrade,
        qty: f64,
        maker: bool,
        fee_rate: f64,
    ) -> ServiceResult<Fill> {
        let quote_amount = qty * trade.price;
        let fee = quote_amount * fee_rate;
        let (_, quote) = split_symbol(&order.symbol, self.accounts.default_quote_asset());

        self.accounts
            .apply_execution(
                order.session_id,
                &order.symbol,
                order.side.clone(),
                Quantity(qty),
                quote_amount,
                fee,
                &quote,
            )
            .await?;

        order.filled_quantity = Quantity(order.filled_quantity.0 + qty);
        order.updated_at = TimestampMs(trade.event_time.0);
        if order.filled_quantity.0 >= order.quantity.0 - f64::EPSILON {
            order.status = OrderStatus::Filled;
        } else {
            order.status = OrderStatus::PartiallyFilled;
        }
        order.maker_taker = Some(if maker {
            Liquidity::Maker
        } else {
            Liquidity::Taker
        });

        Ok(Fill {
            order_id: order.id,
            session_id: order.session_id,
            symbol: order.symbol.clone(),
            trade_id: trade.trade_id,
            price: Price(trade.price),
            qty: Quantity(qty),
            quote_qty: quote_amount,
            fee,
            fee_asset: quote,
            maker,
            event_time: trade.event_time,
        })
    }

    fn should_fill(&self, order: &Order, trade: &AggTrade) -> bool {
        if trade.event_time.0 < order.created_at.0 {
            return false;
        }
        match order.order_type {
            OrderType::Market => true,
            OrderType::Limit => {
                let price = match order.price {
                    Some(price) => price.0,
                    None => return false,
                };
                match order.side {
                    OrderSide::Buy => trade.price <= price,
                    OrderSide::Sell => trade.price >= price,
                }
            }
        }
    }

    fn fill_quantity(&self, remaining: f64, trade_qty: f64) -> f64 {
        if trade_qty <= 0.0 {
            return 0.0;
        }
        if self.fee_config.partial_fills {
            remaining.min(trade_qty)
        } else {
            remaining
        }
    }

    fn is_maker(&self, order: &Order) -> bool {
        match order.order_type {
            OrderType::Market => false,
            OrderType::Limit => match order.maker_taker {
                Some(Liquidity::Taker) => false,
                Some(Liquidity::Maker) => true,
                None => true,
            },
        }
    }

    pub async fn on_session_end(&self, session_id: Uuid) {
        if let Err(err) = self.orders_repo.mark_expired_for_session(session_id).await {
            warn!(%session_id, error = %err, "failed to expire orders on session end");
        }
    }
}
