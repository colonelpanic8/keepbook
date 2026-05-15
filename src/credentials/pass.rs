//! Password-store (pass) credential backend.
//!
//! Retrieves credentials from pass entries. Each entry can contain multiple
//! fields in the format `field-name: value`.

use std::collections::HashMap;
use std::process::Command;

use anyhow::{Context, Result};
use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use super::field_entry::FieldEntry;
use super::CredentialStore;

/// Configuration for a pass credential store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassConfig {
    /// The pass entry path (e.g., "finance/coinbase-api").
    pub path: String,

    /// Mapping from logical key names to field names in the pass entry.
    /// If not specified, the logical key name is used as-is.
    #[serde(default)]
    pub fields: HashMap<String, String>,
}

/// Credential store backed by password-store (pass).
///
/// Reads credentials from a pass entry, parsing fields in the format:
/// ```text
/// field-name: value
/// ```
///
/// The first line of the entry is treated as the "password" field.
pub struct PassCredentialStore {
    config: PassConfig,
}

impl PassCredentialStore {
    /// Create a new pass credential store with the given configuration.
    pub fn new(config: PassConfig) -> Self {
        Self { config }
    }

    /// Create a store for a simple pass entry path, using key names directly as field names.
    pub fn from_path(path: impl Into<String>) -> Self {
        Self::new(PassConfig {
            path: path.into(),
            fields: HashMap::new(),
        })
    }

    /// Get the field name in the pass entry for a logical key.
    fn field_name<'a>(&'a self, key: &'a str) -> &'a str {
        self.config
            .fields
            .get(key)
            .map(|s| s.as_str())
            .unwrap_or(key)
    }

    /// Read and parse the pass entry.
    fn read_entry(&self) -> Result<FieldEntry> {
        let output = Command::new("pass")
            .arg("show")
            .arg(&self.config.path)
            .output()
            .context("Failed to run pass command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("pass command failed: {}", stderr.trim());
        }

        let content = String::from_utf8(output.stdout).context("Invalid UTF-8 in pass output")?;

        Ok(FieldEntry::parse(&content))
    }

    /// Write a field to the pass entry.
    fn write_field(&self, field: &str, value: &str) -> Result<()> {
        // Read existing entry
        let mut entry = self.read_entry().unwrap_or_default();

        // Update the field
        entry.fields.insert(field.to_string(), value.to_string());

        // Reconstruct the entry content
        let content = entry.to_string();

        // Write back using pass insert
        let mut child = Command::new("pass")
            .arg("insert")
            .arg("--multiline")
            .arg("--force")
            .arg(&self.config.path)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn pass command")?;

        use std::io::Write;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(content.as_bytes())
                .context("Failed to write to pass stdin")?;
        }

        let status = child.wait().context("Failed to wait for pass command")?;
        if !status.success() {
            anyhow::bail!("pass insert command failed");
        }

        Ok(())
    }
}

#[async_trait]
impl CredentialStore for PassCredentialStore {
    async fn get(&self, key: &str) -> Result<Option<SecretString>> {
        let field = self.field_name(key);
        let entry = self.read_entry()?;

        Ok(entry
            .fields
            .get(field)
            .map(|v| SecretString::from(v.clone())))
    }

    async fn set(&self, key: &str, value: SecretString) -> Result<()> {
        let field = self.field_name(key);
        self.write_field(field, value.expose_secret())?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/credentials/pass_tests.rs"]
mod pass_tests;
