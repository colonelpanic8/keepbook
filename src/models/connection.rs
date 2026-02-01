use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::credentials::CredentialConfig;

use super::Id;

/// Human-declared connection configuration.
/// Stored in `connection.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Display name for this connection.
    pub name: String,
    /// Which synchronizer plugin to use (e.g., "schwab", "plaid", "coinbase").
    pub synchronizer: String,
    /// Credential configuration for this connection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<CredentialConfig>,
}

/// Machine-managed connection state.
/// Stored in `connection.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionState {
    /// Unique identifier for this connection.
    pub id: Id,
    /// Current connection status.
    pub status: ConnectionStatus,
    /// When this connection was created.
    pub created_at: DateTime<Utc>,
    /// Information about the last sync attempt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<LastSync>,
    /// Account IDs managed by this connection.
    #[serde(default)]
    pub account_ids: Vec<Id>,
    /// Opaque data owned by the synchronizer plugin.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub synchronizer_data: serde_json::Value,
}

impl ConnectionState {
    /// Create a new connection state with default values.
    pub fn new() -> Self {
        Self {
            id: Id::new(),
            status: ConnectionStatus::Active,
            created_at: Utc::now(),
            last_sync: None,
            account_ids: Vec::new(),
            synchronizer_data: serde_json::Value::Null,
        }
    }
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::new()
    }
}

/// A fully loaded connection (config + state).
#[derive(Debug, Clone)]
pub struct Connection {
    pub config: ConnectionConfig,
    pub state: ConnectionState,
}

impl Connection {
    /// Create a new connection from config, generating fresh state.
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            state: ConnectionState::new(),
        }
    }

    /// Convenience accessors
    pub fn id(&self) -> &Id {
        &self.state.id
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    pub fn synchronizer(&self) -> &str {
        &self.config.synchronizer
    }

    pub fn status(&self) -> ConnectionStatus {
        self.state.status
    }
}

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
