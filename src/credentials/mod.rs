//! Credential storage abstraction.
//!
//! Provides a unified interface for retrieving and storing credentials
//! from various backends (pass, age-encrypted files, environment variables, etc.)
//!
//! # Configuration
//!
//! Each connection can have a `credentials.toml` file specifying which backend
//! to use and how to configure it:
//!
//! ```toml
//! backend = "pass"
//! path = "finance/coinbase-api"
//!
//! [fields]
//! key_name = "key-name"
//! private_key = "private-key"
//! ```

mod config;
mod pass;
mod session;

pub use config::CredentialConfig;
pub use pass::{PassConfig, PassCredentialStore};
pub use session::{SessionCache, SessionData, StoredCookie};

use anyhow::Result;
use async_trait::async_trait;
use secrecy::SecretString;

/// A key-value store for credentials.
///
/// Implementations provide access to credentials from various backends.
/// The interface is intentionally simple - just get/set by key name.
/// The synchronizer defines what keys it needs, and the backend configuration
/// maps those keys to backend-specific locations.
#[async_trait]
pub trait CredentialStore: Send + Sync {
    /// Retrieve a credential by key.
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    /// Returns `Err` if there was an error accessing the backend.
    async fn get(&self, key: &str) -> Result<Option<SecretString>>;

    /// Store a credential.
    ///
    /// Used for initial setup flows (e.g., OAuth token exchange) or
    /// updating credentials programmatically.
    ///
    /// Returns `Err` if the backend doesn't support writes or if
    /// there was an error storing the credential.
    async fn set(&self, key: &str, value: SecretString) -> Result<()>;

    /// Check if this store supports writes.
    ///
    /// Some backends (like environment variables) are read-only.
    fn supports_write(&self) -> bool {
        true
    }
}
