use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::clock::{Clock, SystemClock};

use crate::credentials::CredentialConfig;
use crate::duration::deserialize_duration_opt;

use super::{Id, IdGenerator, UuidIdGenerator};

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
    /// Override balance staleness for this connection.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_duration_opt"
    )]
    pub balance_staleness: Option<std::time::Duration>,
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
        Self::new_with_generator(&UuidIdGenerator, &SystemClock)
    }

    pub fn new_with(id: Id, created_at: DateTime<Utc>) -> Self {
        Self {
            id,
            status: ConnectionStatus::Active,
            created_at,
            last_sync: None,
            account_ids: Vec::new(),
            synchronizer_data: serde_json::Value::Null,
        }
    }

    pub fn new_with_generator(ids: &dyn IdGenerator, clock: &dyn Clock) -> Self {
        Self::new_with(ids.new_id(), clock.now())
    }
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use crate::models::{FixedIdGenerator, Id};
    use chrono::TimeZone;

    #[test]
    fn connection_state_new_with_generator_is_deterministic() {
        let fixed_id = Id::from_string("conn-1");
        let ids = FixedIdGenerator::new([fixed_id.clone()]);
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

        let state = ConnectionState::new_with_generator(&ids, &clock);
        assert_eq!(state.id, fixed_id);
        assert_eq!(state.created_at, clock.now());
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

impl ConnectionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionStatus::Active => "active",
            ConnectionStatus::Error => "error",
            ConnectionStatus::Disconnected => "disconnected",
            ConnectionStatus::PendingReauth => "pending_reauth",
        }
    }
}

impl fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
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
