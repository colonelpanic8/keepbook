//! Credential configuration.
//!
//! Defines the format for `credentials.toml` files that specify which
//! credential backend to use and how to configure it.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::age::{AgeConfig, AgeCredentialStore};
use super::env::{EnvConfig, EnvCredentialStore};
use super::pass::{PassConfig, PassCredentialStore};
use super::CredentialStore;

/// Configuration for a credential store.
///
/// This is typically loaded from a `credentials.toml` file in a connection directory.
///
/// # Example
///
/// ```toml
/// backend = "pass"
///
/// [pass]
/// path = "finance/coinbase-api"
///
/// [pass.fields]
/// key_name = "key-name"
/// private_key = "private-key"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "lowercase")]
pub enum CredentialConfig {
    /// Password-store (pass) backend.
    Pass {
        #[serde(flatten)]
        config: PassConfig,
    },

    /// Process environment variable backend.
    Env {
        #[serde(flatten)]
        config: EnvConfig,
    },

    /// age-encrypted file backend.
    Age {
        #[serde(flatten)]
        config: AgeConfig,
    },
    // Vault { ... },
}

impl CredentialConfig {
    /// Load credential configuration from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read credentials config: {}", path.display()))?;

        toml::from_str(&content)
            .with_context(|| format!("Failed to parse credentials config: {}", path.display()))
    }

    /// Load credential configuration from a file, returning None if file doesn't exist.
    pub fn load_optional(path: &Path) -> Result<Option<Self>> {
        if path.exists() {
            Ok(Some(Self::load(path)?))
        } else {
            Ok(None)
        }
    }

    /// Build a credential store from this configuration.
    pub fn build(&self) -> Box<dyn CredentialStore> {
        self.build_with_base_dir(None)
    }

    /// Build a credential store, resolving relative file paths from `base_dir`
    /// for backends that read files.
    pub fn build_with_base_dir(&self, base_dir: Option<&Path>) -> Box<dyn CredentialStore> {
        match self {
            CredentialConfig::Pass { config } => Box::new(PassCredentialStore::new(config.clone())),
            CredentialConfig::Env { config } => Box::new(EnvCredentialStore::new(config.clone())),
            CredentialConfig::Age { config } => match base_dir {
                Some(base_dir) => Box::new(AgeCredentialStore::with_base_dir(
                    config.clone(),
                    base_dir.to_path_buf(),
                )),
                None => Box::new(AgeCredentialStore::new(config.clone())),
            },
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/credentials/config_tests.rs"]
mod config_tests;
