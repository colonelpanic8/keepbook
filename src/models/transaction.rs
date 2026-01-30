use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Asset;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStatus {
    Pending,
    Posted,
    Reversed,
    Canceled,
    Failed,
}

/// A financial transaction. Stored in monthly JSONL files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    /// Signed amount as string - negative for debits, positive for credits
    pub amount: String,
    pub asset: Asset,
    /// Raw description from the source
    pub description: String,
    pub status: TransactionStatus,
    /// Opaque data for deduplication, original IDs, etc.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub synchronizer_data: serde_json::Value,
}

impl Transaction {
    pub fn new(
        amount: impl Into<String>,
        asset: Asset,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            amount: amount.into(),
            asset,
            description: description.into(),
            status: TransactionStatus::Posted,
            synchronizer_data: serde_json::Value::Null,
        }
    }

    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    pub fn with_status(mut self, status: TransactionStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_id(mut self, id: Uuid) -> Self {
        self.id = id;
        self
    }

    pub fn with_synchronizer_data(mut self, data: serde_json::Value) -> Self {
        self.synchronizer_data = data;
        self
    }
}
