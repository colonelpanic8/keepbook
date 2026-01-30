use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An individual financial account (checking, savings, credit card, brokerage, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub name: String,
    pub connection_id: Uuid,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub active: bool,
    /// Opaque data owned by the synchronizer plugin
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub synchronizer_data: serde_json::Value,
}

impl Account {
    pub fn new(name: impl Into<String>, connection_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            connection_id,
            tags: Vec::new(),
            created_at: Utc::now(),
            active: true,
            synchronizer_data: serde_json::Value::Null,
        }
    }
}
