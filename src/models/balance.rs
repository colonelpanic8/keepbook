use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::clock::Clock;

use super::Asset;

/// A single asset's balance without timestamp (belongs to a snapshot).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBalance {
    pub asset: Asset,
    /// Amount as string to avoid floating point precision issues
    pub amount: String,
    /// Optional total cost basis for this holding, denominated in the
    /// portfolio/reporting currency used for gain estimates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_basis: Option<String>,
}

impl AssetBalance {
    pub fn new(asset: Asset, amount: impl Into<String>) -> Self {
        Self {
            asset,
            amount: amount.into(),
            cost_basis: None,
        }
    }

    pub fn with_cost_basis(mut self, cost_basis: impl Into<String>) -> Self {
        self.cost_basis = Some(cost_basis.into());
        self
    }
}

/// A point-in-time snapshot of ALL holdings in an account.
/// One line in the JSONL file = one complete account state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceSnapshot {
    pub timestamp: DateTime<Utc>,
    pub balances: Vec<AssetBalance>,
}

impl BalanceSnapshot {
    pub fn new(timestamp: DateTime<Utc>, balances: Vec<AssetBalance>) -> Self {
        Self {
            timestamp,
            balances,
        }
    }

    pub fn now(balances: Vec<AssetBalance>) -> Self {
        Self::new(Utc::now(), balances)
    }

    pub fn now_with(clock: &dyn Clock, balances: Vec<AssetBalance>) -> Self {
        Self::new(clock.now(), balances)
    }
}
