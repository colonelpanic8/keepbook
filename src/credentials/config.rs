//! Credential configuration.
//!
//! Defines the format for `credentials.toml` files that specify which
//! credential backend to use and how to configure it.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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
    // Future backends:
    // Env { ... },
    // Age { ... },
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
        match self {
            CredentialConfig::Pass { config } => {
                Box::new(PassCredentialStore::new(config.clone()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_pass_config() -> Result<()> {
        let mut file = NamedTempFile::new()?;
        writeln!(
            file,
            r#"
backend = "pass"
path = "finance/coinbase-api"

[fields]
key_name = "key-name"
private_key = "private-key"
"#
        )?;

        let config = CredentialConfig::load(file.path())?;

        match config {
            CredentialConfig::Pass { config } => {
                assert_eq!(config.path, "finance/coinbase-api");
                assert_eq!(config.fields.get("key_name"), Some(&"key-name".to_string()));
                assert_eq!(
                    config.fields.get("private_key"),
                    Some(&"private-key".to_string())
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_parse_minimal_pass_config() -> Result<()> {
        let mut file = NamedTempFile::new()?;
        writeln!(
            file,
            r#"
backend = "pass"
path = "my-api-key"
"#
        )?;

        let config = CredentialConfig::load(file.path())?;

        match config {
            CredentialConfig::Pass { config } => {
                assert_eq!(config.path, "my-api-key");
                assert!(config.fields.is_empty());
            }
        }

        Ok(())
    }
}
