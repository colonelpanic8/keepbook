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
#[serde(from = "ProposedTransactionEditSerde")]
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
}

#[derive(Debug, Deserialize)]
struct ProposedTransactionEditSerde {
    id: Id,
    account_id: Id,
    transaction_id: Id,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    status: ProposedTransactionEditStatus,

    #[serde(default, deserialize_with = "deserialize_patch_field")]
    description: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    note: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_string_or_vec_field")]
    category: Option<Option<Vec<String>>>,
    #[serde(default, deserialize_with = "deserialize_patch_string_or_vec_field")]
    subcategory: Option<Option<Vec<String>>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    tags: Option<Option<Vec<String>>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    subtags: Option<Option<Vec<String>>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    effective_date: Option<Option<NaiveDate>>,
}

impl From<ProposedTransactionEditSerde> for ProposedTransactionEdit {
    fn from(value: ProposedTransactionEditSerde) -> Self {
        Self {
            id: value.id,
            account_id: value.account_id,
            transaction_id: value.transaction_id,
            created_at: value.created_at,
            updated_at: value.updated_at,
            status: value.status,
            description: value.description,
            note: value.note,
            tags: value.tags.or(value.category),
            subtags: value.subtags.or(value.subcategory),
            effective_date: value.effective_date,
        }
    }
}

fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::<T>::deserialize(deserializer)?))
}

fn deserialize_patch_string_or_vec_field<'de, D>(
    deserializer: D,
) -> Result<Option<Option<Vec<String>>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        String(String),
        Vec(Vec<String>),
    }

    Ok(Some(
        match Option::<StringOrVec>::deserialize(deserializer)? {
            None => None,
            Some(StringOrVec::String(value)) => Some(vec![value]),
            Some(StringOrVec::Vec(values)) => Some(values),
        },
    ))
}

impl ProposedTransactionEdit {
    pub fn has_edit(&self) -> bool {
        self.description.is_some()
            || self.note.is_some()
            || self.tags.is_some()
            || self.subtags.is_some()
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
            tags: self.tags.clone(),
            subtags: self.subtags.clone(),
            effective_date: self.effective_date,
        }
    }
}
