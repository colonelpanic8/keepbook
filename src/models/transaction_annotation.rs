use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use super::Id;

/// Legacy magic tags that mark a transaction as ignored from spending.
pub const SPENDING_IGNORE_TAGS: [&str; 3] =
    ["ignore_spending", "ignore-spending", "ignore:spending"];

/// Whether a tag is one of the legacy magic spending-ignore tags.
pub fn tag_ignores_spending(tag: &str) -> bool {
    let normalized = tag.trim().to_lowercase();
    SPENDING_IGNORE_TAGS.contains(&normalized.as_str())
}

/// Current (materialized) annotation state for a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionAnnotation {
    pub transaction_id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignore_spending: Option<bool>,
}

impl TransactionAnnotation {
    pub fn new(transaction_id: Id) -> Self {
        Self {
            transaction_id,
            description: None,
            note: None,
            tags: None,
            subtags: None,
            effective_date: None,
            ignore_spending: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.description.is_none()
            && self.note.is_none()
            && self.tags.is_none()
            && self.subtags.is_none()
            && self.effective_date.is_none()
            && self.ignore_spending.is_none()
    }

    /// Whether this annotation marks the transaction as ignored from spending,
    /// either via the explicit `ignore_spending` flag or a legacy magic tag.
    pub fn ignores_spending(&self) -> bool {
        self.ignore_spending == Some(true)
            || self
                .tags
                .as_ref()
                .is_some_and(|tags| tags.iter().any(|tag| tag_ignores_spending(tag)))
    }
}

/// Append-only transaction annotation patch.
///
/// Each field is tri-state:
/// - outer `None`: field not mentioned (no change)
/// - `Some(None)`: field explicitly cleared (JSON null)
/// - `Some(Some(v))`: field set/overwritten
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionAnnotationPatch {
    pub transaction_id: Id,
    pub timestamp: DateTime<Utc>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_patch_field"
    )]
    pub description: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_patch_field"
    )]
    pub note: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_patch_field"
    )]
    pub tags: Option<Option<Vec<String>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_patch_field"
    )]
    pub subtags: Option<Option<Vec<String>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_patch_field"
    )]
    pub effective_date: Option<Option<NaiveDate>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_patch_field"
    )]
    pub ignore_spending: Option<Option<bool>>,
}

fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

impl TransactionAnnotationPatch {
    pub fn apply_to(&self, ann: &mut TransactionAnnotation) {
        if let Some(v) = &self.description {
            ann.description = v.clone();
        }
        if let Some(v) = &self.note {
            ann.note = v.clone();
        }
        if let Some(v) = &self.tags {
            ann.tags = v.clone();
        }
        if let Some(v) = &self.subtags {
            ann.subtags = v.clone();
        }
        if let Some(v) = &self.effective_date {
            ann.effective_date = *v;
        }
        if let Some(v) = &self.ignore_spending {
            ann.ignore_spending = *v;
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/models/transaction_annotation_tests.rs"]
mod transaction_annotation_tests;
