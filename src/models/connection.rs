use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Active,
    Error,
    Disconnected,
    PendingReauth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastSync {
    pub at: DateTime<Utc>,
    pub status: SyncStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    Success,
    Failed,
    Partial,
}

/// A connection represents a sync source (e.g., Plaid link, bank login, API connection).
/// One connection can produce multiple accounts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: Uuid,
    pub name: String,
    pub synchronizer: String,
    pub status: ConnectionStatus,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<LastSync>,
    pub account_ids: Vec<Uuid>,
    /// Opaque data owned by the synchronizer plugin
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub synchronizer_data: serde_json::Value,
}

impl Connection {
    pub fn new(name: impl Into<String>, synchronizer: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            synchronizer: synchronizer.into(),
            status: ConnectionStatus::Active,
            created_at: Utc::now(),
            last_sync: None,
            account_ids: Vec::new(),
            synchronizer_data: serde_json::Value::Null,
        }
    }
}
