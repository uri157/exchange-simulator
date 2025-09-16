use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::domain::models::{AccountSnapshot, Balance};

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BalanceResponse {
    pub asset: String,
    pub free: f64,
    pub locked: f64,
}

impl From<Balance> for BalanceResponse {
    fn from(value: Balance) -> Self {
        Self {
            asset: value.asset,
            free: value.free.0,
            locked: value.locked.0,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountResponse {
    pub session_id: String,
    pub balances: Vec<BalanceResponse>,
}

impl From<AccountSnapshot> for AccountResponse {
    fn from(value: AccountSnapshot) -> Self {
        Self {
            session_id: value.session_id.to_string(),
            balances: value.balances.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountQuery {
    pub session_id: Uuid,
}
