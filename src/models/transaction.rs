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

    fn fill_missing_from(&mut self, other: TransactionStandardizedMetadata) {
        if self.merchant_name.is_none() {
            self.merchant_name = other.merchant_name;
        }
        if self.merchant_category_code.is_none() {
            self.merchant_category_code = other.merchant_category_code;
        }
        if self.merchant_category_label.is_none() {
            self.merchant_category_label = other.merchant_category_label;
        }
        if self.transaction_kind.is_none() {
            self.transaction_kind = other.transaction_kind;
        }
        if self.is_internal_transfer_hint.is_none() {
            self.is_internal_transfer_hint = other.is_internal_transfer_hint;
        }
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
    /// Provider-agnostic metadata used for virtual tags and rule matching.
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
        self.merge_backfilled_standardized_metadata();
        self
    }

    pub fn with_standardized_metadata(mut self, data: TransactionStandardizedMetadata) -> Self {
        self.standardized_metadata = if data.is_empty() { None } else { Some(data) };
        self
    }

    pub fn backfill_standardized_metadata(mut self) -> Self {
        self.merge_backfilled_standardized_metadata();
        self
    }

    fn merge_backfilled_standardized_metadata(&mut self) {
        let Some(derived) =
            derive_standardized_metadata_from_synchronizer_data(&self.synchronizer_data)
        else {
            return;
        };
        match self.standardized_metadata.as_mut() {
            Some(existing) => existing.fill_missing_from(derived),
            None => self.standardized_metadata = Some(derived),
        }
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

fn normalize_category_label(raw: &str) -> Option<String> {
    let normalized = raw.trim().replace(['_', '-'], " ");
    if normalized.is_empty() {
        return None;
    }
    let words: Vec<String> = normalized
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase()),
                None => String::new(),
            }
        })
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
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
    let merchant_category_label = non_empty_str(value, "merchant_category_name").or_else(|| {
        non_empty_str(value, "etu_standard_expense_category_code")
            .and_then(|v| normalize_category_label(&v))
    });
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
#[path = "../../tests/unit/models/transaction_tests.rs"]
mod transaction_tests;
