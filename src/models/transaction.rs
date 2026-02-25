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

/// Provider-agnostic metadata derived from transaction source data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TransactionStandardizedMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merchant_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merchant_category_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merchant_category_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_internal_transfer_hint: Option<bool>,
}

impl TransactionStandardizedMetadata {
    pub fn is_empty(&self) -> bool {
        self.merchant_name.is_none()
            && self.merchant_category_code.is_none()
            && self.merchant_category_label.is_none()
            && self.transaction_kind.is_none()
            && self.is_internal_transfer_hint.is_none()
    }
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
    /// Provider-agnostic metadata used for categorization/rule matching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standardized_metadata: Option<TransactionStandardizedMetadata>,
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
            standardized_metadata: None,
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
        if self.standardized_metadata.is_none() {
            self.standardized_metadata =
                derive_standardized_metadata_from_synchronizer_data(&self.synchronizer_data);
        }
        self
    }

    pub fn with_standardized_metadata(mut self, data: TransactionStandardizedMetadata) -> Self {
        self.standardized_metadata = if data.is_empty() { None } else { Some(data) };
        self
    }

    pub fn backfill_standardized_metadata(mut self) -> Self {
        if self.standardized_metadata.is_none() {
            self.standardized_metadata =
                derive_standardized_metadata_from_synchronizer_data(&self.synchronizer_data);
        }
        self
    }
}

fn non_empty_str(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn first_non_empty_str(values: &serde_json::Value, key: &str) -> Option<String> {
    values
        .get(key)
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(str::trim)
                .find(|s| !s.is_empty())
        })
        .map(|s| s.to_string())
}

fn normalize_transaction_kind(raw: &str) -> Option<String> {
    let value = raw.trim().to_lowercase();
    if value.is_empty() {
        return None;
    }
    if value.contains("purchase") {
        return Some("purchase".to_string());
    }
    if value.contains("payment") {
        return Some("payment".to_string());
    }
    if value.contains("transfer") {
        return Some("transfer".to_string());
    }
    if value.contains("fee") {
        return Some("fee".to_string());
    }
    if value.contains("interest") {
        return Some("interest".to_string());
    }
    if value.contains("refund") {
        return Some("refund".to_string());
    }
    if value.contains("deposit") {
        return Some("deposit".to_string());
    }
    if value.contains("withdraw") {
        return Some("withdrawal".to_string());
    }
    None
}

pub fn derive_standardized_metadata_from_synchronizer_data(
    value: &serde_json::Value,
) -> Option<TransactionStandardizedMetadata> {
    if value.is_null() || !value.is_object() {
        return None;
    }

    let merchant_name = first_non_empty_str(value, "enriched_merchant_names")
        .or_else(|| non_empty_str(value, "merchant_dba_name"))
        .or_else(|| non_empty_str(value, "merchant_name"));
    let merchant_category_code = non_empty_str(value, "merchant_category_code");
    let merchant_category_label = non_empty_str(value, "merchant_category_name");
    let transaction_kind = non_empty_str(value, "etu_standard_transaction_type_group_name")
        .or_else(|| non_empty_str(value, "etu_standard_transaction_type_name"))
        .and_then(|v| normalize_transaction_kind(&v));
    let is_internal_transfer_hint = transaction_kind
        .as_deref()
        .map(|kind| matches!(kind, "transfer" | "payment"));

    let metadata = TransactionStandardizedMetadata {
        merchant_name,
        merchant_category_code,
        merchant_category_label,
        transaction_kind,
        is_internal_transfer_hint,
    };
    if metadata.is_empty() {
        None
    } else {
        Some(metadata)
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

        let tx =
            Transaction::new_with_generator(&ids, &clock, "-1", Asset::currency("USD"), "Test");

        assert_eq!(tx.id.as_str(), "tx-1");
        assert_eq!(tx.timestamp, clock.now());
    }

    #[test]
    fn with_synchronizer_data_derives_standardized_metadata_for_chase_fields() {
        let tx = Transaction::new("-10", Asset::currency("USD"), "Coffee").with_synchronizer_data(
            serde_json::json!({
                "merchant_dba_name": "Coffee Shop",
                "merchant_category_code": "5814",
                "merchant_category_name": "Fast Food",
                "etu_standard_transaction_type_group_name": "Purchases",
                "enriched_merchant_names": ["Blue Bottle Coffee"],
            }),
        );

        let md = tx.standardized_metadata.expect("expected metadata");
        assert_eq!(md.merchant_name.as_deref(), Some("Blue Bottle Coffee"));
        assert_eq!(md.merchant_category_code.as_deref(), Some("5814"));
        assert_eq!(md.merchant_category_label.as_deref(), Some("Fast Food"));
        assert_eq!(md.transaction_kind.as_deref(), Some("purchase"));
        assert_eq!(md.is_internal_transfer_hint, Some(false));
    }

    #[test]
    fn backfill_standardized_metadata_populates_when_missing() {
        let tx = Transaction {
            id: Id::from_string("tx-1"),
            timestamp: Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap(),
            amount: "-10".to_string(),
            asset: Asset::currency("USD"),
            description: "Test".to_string(),
            status: TransactionStatus::Posted,
            synchronizer_data: serde_json::json!({
                "merchant_dba_name": "Coffee Shop",
                "merchant_category_code": "5814",
            }),
            standardized_metadata: None,
        }
        .backfill_standardized_metadata();

        let md = tx.standardized_metadata.expect("expected metadata");
        assert_eq!(md.merchant_name.as_deref(), Some("Coffee Shop"));
        assert_eq!(md.merchant_category_code.as_deref(), Some("5814"));
        assert_eq!(md.transaction_kind, None);
    }
}
