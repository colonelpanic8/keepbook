use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Id;

/// An individual financial account (checking, savings, credit card, brokerage, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Id,
    pub name: String,
    pub connection_id: Id,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub active: bool,
    /// Opaque data owned by the synchronizer plugin
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub synchronizer_data: serde_json::Value,
}

impl Account {
    pub fn new(name: impl Into<String>, connection_id: Id) -> Self {
        Self {
            id: Id::new(),
            name: name.into(),
            connection_id,
            tags: Vec::new(),
            created_at: Utc::now(),
            active: true,
            synchronizer_data: serde_json::Value::Null,
        }
    }
}
