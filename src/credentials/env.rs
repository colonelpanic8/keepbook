use std::collections::HashMap;

use anyhow::{bail, Result};
use async_trait::async_trait;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use super::CredentialStore;

/// Configuration for an environment-variable credential store.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvConfig {
    /// Optional prefix for keys that do not have an explicit field mapping.
    #[serde(default)]
    pub prefix: Option<String>,

    /// Mapping from logical key names to environment variable names.
    #[serde(default)]
    pub fields: HashMap<String, String>,
}

/// Read-only credential store backed by process environment variables.
pub struct EnvCredentialStore {
    config: EnvConfig,
}

impl EnvCredentialStore {
    pub fn new(config: EnvConfig) -> Self {
        Self { config }
    }

    fn env_name(&self, key: &str) -> String {
        self.config.fields.get(key).cloned().unwrap_or_else(|| {
            let normalized: String = key
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() {
                        ch.to_ascii_uppercase()
                    } else {
                        '_'
                    }
                })
                .collect();
            match self.config.prefix.as_deref() {
                Some(prefix) => format!("{prefix}{normalized}"),
                None => normalized,
            }
        })
    }
}

#[async_trait]
impl CredentialStore for EnvCredentialStore {
    async fn get(&self, key: &str) -> Result<Option<SecretString>> {
        Ok(std::env::var(self.env_name(key))
            .ok()
            .filter(|value| !value.is_empty())
            .map(SecretString::from))
    }

    async fn set(&self, _key: &str, _value: SecretString) -> Result<()> {
        bail!("environment credential store is read-only")
    }

    fn supports_write(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "../../tests/unit/credentials/env_tests.rs"]
mod env_tests;
