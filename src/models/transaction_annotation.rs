use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use super::Id;

/// Current (materialized) annotation state for a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "TransactionAnnotationSerde")]
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
}

#[derive(Debug, Deserialize)]
struct TransactionAnnotationSerde {
    transaction_id: Id,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string_or_vec")]
    category: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_string_or_vec")]
    subcategory: Option<Vec<String>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    subtags: Option<Vec<String>>,
    #[serde(default)]
    effective_date: Option<NaiveDate>,
}

impl From<TransactionAnnotationSerde> for TransactionAnnotation {
    fn from(value: TransactionAnnotationSerde) -> Self {
        Self {
            transaction_id: value.transaction_id,
            description: value.description,
            note: value.note,
            tags: value.tags.or(value.category),
            subtags: value.subtags.or(value.subcategory),
            effective_date: value.effective_date,
        }
    }
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
        }
    }

    pub fn is_empty(&self) -> bool {
        self.description.is_none()
            && self.note.is_none()
            && self.tags.is_none()
            && self.subtags.is_none()
            && self.effective_date.is_none()
    }
}

/// Append-only transaction annotation patch.
///
/// Each field is tri-state:
/// - outer `None`: field not mentioned (no change)
/// - `Some(None)`: field explicitly cleared (JSON null)
/// - `Some(Some(v))`: field set/overwritten
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "TransactionAnnotationPatchSerde")]
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
}

#[derive(Debug, Deserialize)]
struct TransactionAnnotationPatchSerde {
    transaction_id: Id,
    timestamp: DateTime<Utc>,

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

impl From<TransactionAnnotationPatchSerde> for TransactionAnnotationPatch {
    fn from(value: TransactionAnnotationPatchSerde) -> Self {
        Self {
            transaction_id: value.transaction_id,
            timestamp: value.timestamp,
            description: value.description,
            note: value.note,
            tags: value.tags.or(value.category),
            subtags: value.subtags.or(value.subcategory),
            effective_date: value.effective_date,
        }
    }
}

fn deserialize_optional_string_or_vec<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        String(String),
        Vec(Vec<String>),
    }

    Ok(match Option::<StringOrVec>::deserialize(deserializer)? {
        None => None,
        Some(StringOrVec::String(value)) => Some(vec![value]),
        Some(StringOrVec::Vec(values)) => Some(values),
    })
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Id;
    use chrono::TimeZone;

    #[test]
    fn patch_tristate_semantics_apply() {
        let tx_id = Id::from_string("tx-1");
        let mut ann = TransactionAnnotation::new(tx_id.clone());

        let set_note = TransactionAnnotationPatch {
            transaction_id: tx_id.clone(),
            timestamp: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap(),
            description: None,
            note: Some(Some("hello".to_string())),
            tags: None,
            subtags: None,
            effective_date: None,
        };
        set_note.apply_to(&mut ann);
        assert_eq!(ann.note, Some("hello".to_string()));

        let clear_note = TransactionAnnotationPatch {
            transaction_id: tx_id,
            timestamp: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 1).unwrap(),
            description: None,
            note: Some(None),
            tags: None,
            subtags: None,
            effective_date: None,
        };
        clear_note.apply_to(&mut ann);
        assert_eq!(ann.note, None);
    }

    #[test]
    fn patch_json_null_round_trips_as_explicit_clear() {
        let patch: TransactionAnnotationPatch = serde_json::from_str(
            r#"{"transaction_id":"tx-1","timestamp":"2024-02-01T00:00:00Z","description":null}"#,
        )
        .unwrap();
        assert_eq!(patch.description, Some(None));
        assert_eq!(patch.note, None);
    }

    #[test]
    fn legacy_category_fields_deserialize_as_tags_and_subtags() {
        let patch: TransactionAnnotationPatch = serde_json::from_str(
            r#"{"transaction_id":"tx-1","timestamp":"2024-02-01T00:00:00Z","category":"food","subcategory":"coffee"}"#,
        )
        .unwrap();
        assert_eq!(patch.tags, Some(Some(vec!["food".to_string()])));
        assert_eq!(patch.subtags, Some(Some(vec!["coffee".to_string()])));
    }
}
