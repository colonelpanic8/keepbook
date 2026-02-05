use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::clock::{Clock, SystemClock};

use super::{Asset, Id, IdGenerator, UuidIdGenerator};

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
    pub id: Id,
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
    pub fn new(amount: impl Into<String>, asset: Asset, description: impl Into<String>) -> Self {
        Self::new_with_generator(&UuidIdGenerator, &SystemClock, amount, asset, description)
    }

    pub fn new_with_generator(
        ids: &dyn IdGenerator,
        clock: &dyn Clock,
        amount: impl Into<String>,
        asset: Asset,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: ids.new_id(),
            timestamp: clock.now(),
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

    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    pub fn with_synchronizer_data(mut self, data: serde_json::Value) -> Self {
        self.synchronizer_data = data;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use crate::models::{FixedIdGenerator, Id};
    use chrono::TimeZone;

    #[test]
    fn transaction_new_with_generator_is_deterministic() {
        let ids = FixedIdGenerator::new([Id::from_string("tx-1")]);
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

        let tx = Transaction::new_with_generator(
            &ids,
            &clock,
            "-1",
            Asset::currency("USD"),
            "Test",
        );

        assert_eq!(tx.id.as_str(), "tx-1");
        assert_eq!(tx.timestamp, clock.now());
    }
}
