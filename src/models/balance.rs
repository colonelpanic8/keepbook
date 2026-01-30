use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Asset;

/// A point-in-time snapshot of an account's holdings.
/// Stored in monthly JSONL files within the account directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub timestamp: DateTime<Utc>,
    pub asset: Asset,
    /// Amount as string to avoid floating point precision issues
    pub amount: String,
}

impl Balance {
    pub fn new(asset: Asset, amount: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            asset,
            amount: amount.into(),
        }
    }

    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }
}
