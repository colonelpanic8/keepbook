use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecurringTransactionReviewStatus {
    Verified,
    Dismissed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurringTransactionReviewOccurrence {
    pub account_id: Id,
    pub transaction_id: Id,
}

/// Append-only review event for a detected recurring transaction candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurringTransactionReview {
    pub candidate_key: String,
    pub updated_at: DateTime<Utc>,
    pub status: RecurringTransactionReviewStatus,
    pub name: String,
    pub normalized_name: String,
    pub cadence: String,
    pub amount_typical: String,
    pub asset: serde_json::Value,
    pub occurrences: Vec<RecurringTransactionReviewOccurrence>,
}
