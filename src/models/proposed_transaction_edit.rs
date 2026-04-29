use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use super::{Id, TransactionAnnotationPatch};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposedTransactionEditStatus {
    Pending,
    Approved,
    Rejected,
    Removed,
}

/// A queued transaction annotation edit.
///
/// The editable fields have the same tri-state semantics as
/// `TransactionAnnotationPatch`: absent means no change, null means clear, and a
/// value means set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposedTransactionEdit {
    pub id: Id,
    pub account_id: Id,
    pub transaction_id: Id,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: ProposedTransactionEditStatus,

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
    pub category: Option<Option<String>>,
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
    pub effective_date: Option<Option<NaiveDate>>,
}

fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

impl ProposedTransactionEdit {
    pub fn has_edit(&self) -> bool {
        self.description.is_some()
            || self.note.is_some()
            || self.category.is_some()
            || self.tags.is_some()
            || self.effective_date.is_some()
    }

    pub fn with_status(&self, status: ProposedTransactionEditStatus, now: DateTime<Utc>) -> Self {
        Self {
            status,
            updated_at: now,
            ..self.clone()
        }
    }

    pub fn to_annotation_patch(&self, timestamp: DateTime<Utc>) -> TransactionAnnotationPatch {
        TransactionAnnotationPatch {
            transaction_id: self.transaction_id.clone(),
            timestamp,
            description: self.description.clone(),
            note: self.note.clone(),
            category: self.category.clone(),
            tags: self.tags.clone(),
            effective_date: self.effective_date,
        }
    }
}
