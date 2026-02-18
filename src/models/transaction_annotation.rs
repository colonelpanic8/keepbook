use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Id;

/// Current (materialized) annotation state for a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionAnnotation {
    pub transaction_id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl TransactionAnnotation {
    pub fn new(transaction_id: Id) -> Self {
        Self {
            transaction_id,
            description: None,
            note: None,
            category: None,
            tags: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.description.is_none()
            && self.note.is_none()
            && self.category.is_none()
            && self.tags.is_none()
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

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Option<Vec<String>>>,
}

impl TransactionAnnotationPatch {
    pub fn apply_to(&self, ann: &mut TransactionAnnotation) {
        if let Some(v) = &self.description {
            ann.description = v.clone();
        }
        if let Some(v) = &self.note {
            ann.note = v.clone();
        }
        if let Some(v) = &self.category {
            ann.category = v.clone();
        }
        if let Some(v) = &self.tags {
            ann.tags = v.clone();
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
            category: None,
            tags: None,
        };
        set_note.apply_to(&mut ann);
        assert_eq!(ann.note, Some("hello".to_string()));

        let clear_note = TransactionAnnotationPatch {
            transaction_id: tx_id,
            timestamp: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 1).unwrap(),
            description: None,
            note: Some(None),
            category: None,
            tags: None,
        };
        clear_note.apply_to(&mut ann);
        assert_eq!(ann.note, None);
    }
}
