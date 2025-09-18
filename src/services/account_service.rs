use std::sync::Arc;

use indexmap::IndexMap;
use uuid::Uuid;

use crate::domain::{
    models::{AccountSnapshot, Balance, OrderSide},
    traits::AccountsRepo,
    value_objects::{Price, Quantity},
};

use super::ServiceResult;

pub struct AccountService {
    repo: Arc<dyn AccountsRepo>,
    default_quote: String,
    initial_quote_balance: f64,
}

impl AccountService {
    pub fn new(
        repo: Arc<dyn AccountsRepo>,
        default_quote: String,
        initial_quote_balance: f64,
    ) -> Self {
        Self {
            repo,
            default_quote,
            initial_quote_balance,
        }
    }

    pub async fn ensure_session_account(&self, session_id: Uuid) -> ServiceResult<()> {
        match self.repo.get_account(session_id).await {
            Ok(_) => Ok(()),
            Err(_) => {
                let snapshot = AccountSnapshot {
                    session_id,
                    balances: vec![Balance {
                        asset: self.default_quote.clone(),
                        free: Quantity(self.initial_quote_balance),
                        locked: Quantity::default(),
                    }],
                };
                self.repo.save_account(snapshot).await
            }
        }
    }

    pub async fn get_account(&self, session_id: Uuid) -> ServiceResult<AccountSnapshot> {
        self.repo.get_account(session_id).await
    }

    pub async fn apply_fill(
        &self,
        session_id: Uuid,
        symbol: &str,
        side: OrderSide,
        price: Price,
        quantity: Quantity,
    ) -> ServiceResult<AccountSnapshot> {
        let mut snapshot = self.repo.get_account(session_id).await?;
        let (base, quote) = split_symbol(symbol, &self.default_quote);

        let mut balances: IndexMap<String, Balance> = snapshot
            .balances
            .into_iter()
            .map(|b| (b.asset.clone(), b))
            .collect();

        let trade_value = price.0 * quantity.0;

        match side {
            OrderSide::Buy => {
                // quote --
                {
                    let q = balances.entry(quote.clone()).or_insert_with(|| Balance {
                        asset: quote.clone(),
                        free: Quantity::default(),
                        locked: Quantity::default(),
                    });
                    q.free = Quantity(q.free.0 - trade_value);
                }
                // base ++
                {
                    let b = balances.entry(base.clone()).or_insert_with(|| Balance {
                        asset: base.clone(),
                        free: Quantity::default(),
                        locked: Quantity::default(),
                    });
                    b.free = Quantity(b.free.0 + quantity.0);
                }
            }
            OrderSide::Sell => {
                // base --
                {
                    let b = balances.entry(base.clone()).or_insert_with(|| Balance {
                        asset: base.clone(),
                        free: Quantity::default(),
                        locked: Quantity::default(),
                    });
                    b.free = Quantity(b.free.0 - quantity.0);
                }
                // quote ++
                {
                    let q = balances.entry(quote.clone()).or_insert_with(|| Balance {
                        asset: quote.clone(),
                        free: Quantity::default(),
                        locked: Quantity::default(),
                    });
                    q.free = Quantity(q.free.0 + trade_value);
                }
            }
        }

        snapshot.balances = balances.into_iter().map(|(_, b)| b).collect();
        self.repo.save_account(snapshot.clone()).await?;
        Ok(snapshot)
    }
}

fn split_symbol(symbol: &str, default_quote: &str) -> (String, String) {
    const COMMON_QUOTES: [&str; 6] = ["USDT", "USD", "BUSD", "USDC", "BTC", "ETH"];

    for quote in COMMON_QUOTES.iter().chain(std::iter::once(&default_quote)) {
        if let Some(base) = symbol.strip_suffix(*quote) {
            if !base.is_empty() {
                return (base.to_string(), (*quote).to_string());
            }
        }
    }

    let split = symbol.len() / 2;
    (symbol[..split].to_string(), symbol[split..].to_string())
}
