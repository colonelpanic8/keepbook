use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Asset;

/// A single asset's balance without timestamp (belongs to a snapshot).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBalance {
    pub asset: Asset,
    /// Amount as string to avoid floating point precision issues
    pub amount: String,
}

impl AssetBalance {
    pub fn new(asset: Asset, amount: impl Into<String>) -> Self {
        Self {
            asset,
            amount: amount.into(),
        }
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
}
