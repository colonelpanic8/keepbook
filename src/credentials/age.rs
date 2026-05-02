use std::collections::HashMap;
use std::io::BufReader;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use super::field_entry::FieldEntry;
use super::CredentialStore;

const AGE_IDENTITY_PATH_ENV: &str = "KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH";

/// Configuration for an age-encrypted credential entry.
///
/// `path` points to an age file whose decrypted payload uses the same multiline
/// field format as a pass entry:
///
/// ```text
/// optional-first-line-password
/// key-name: value
/// private-key: escaped\nmultiline\nvalue
/// ```
///
/// The identity should usually be the same SSH private key used to clone the
/// mobile data repo. It can be set per connection with `identity_path`, or
/// dynamically via `KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgeConfig {
    /// Path to the age-encrypted credential file.
    pub path: String,

    /// Optional path to an SSH private key identity.
    #[serde(default)]
    pub identity_path: Option<String>,

    /// Mapping from logical key names to field names in the decrypted entry.
    #[serde(default)]
    pub fields: HashMap<String, String>,
}

/// Read-only credential store backed by an age-encrypted file.
pub struct AgeCredentialStore {
    config: AgeConfig,
    base_dir: Option<PathBuf>,
}

impl AgeCredentialStore {
    pub fn new(config: AgeConfig) -> Self {
        Self {
            config,
            base_dir: None,
        }
    }

    pub fn with_base_dir(config: AgeConfig, base_dir: impl Into<PathBuf>) -> Self {
        Self {
            config,
            base_dir: Some(base_dir.into()),
        }
    }

    fn field_name<'a>(&'a self, key: &'a str) -> &'a str {
        self.config
            .fields
            .get(key)
            .map(|s| s.as_str())
            .unwrap_or(key)
    }

    fn resolve_path(&self, configured: &str) -> PathBuf {
        let path = PathBuf::from(configured);
        if path.is_absolute() {
            path
        } else {
            self.base_dir
                .as_ref()
                .map(|base| base.join(&path))
                .unwrap_or(path)
        }
    }

    fn identity_path(&self) -> Result<PathBuf> {
        if let Some(path) = self
            .config
            .identity_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            return Ok(self.resolve_path(path));
        }

        let path = std::env::var(AGE_IDENTITY_PATH_ENV).with_context(|| {
            format!(
                "age credential identity is not configured; set identity_path or {AGE_IDENTITY_PATH_ENV}"
            )
        })?;
        Ok(self.resolve_path(path.trim()))
    }

    fn read_entry(&self) -> Result<FieldEntry> {
        let age_path = self.resolve_path(&self.config.path);
        let identity_path = self.identity_path()?;
        let ciphertext = std::fs::read(&age_path)
            .with_context(|| format!("Failed to read age credentials {}", age_path.display()))?;
        let identity_pem = std::fs::read_to_string(&identity_path).with_context(|| {
            format!(
                "Failed to read age SSH identity {}",
                identity_path.display()
            )
        })?;
        let reader = BufReader::new(identity_pem.as_bytes());
        let identity =
            ::age::ssh::Identity::from_buffer(reader, Some(identity_path.display().to_string()))
                .with_context(|| {
                    format!("Failed to parse SSH identity {}", identity_path.display())
                })?;

        let plaintext = ::age::decrypt(&identity, &ciphertext)
            .with_context(|| format!("Failed to decrypt {}", age_path.display()))?;
        let content = String::from_utf8(plaintext).with_context(|| {
            format!(
                "Decrypted credentials are not UTF-8: {}",
                age_path.display()
            )
        })?;
        Ok(FieldEntry::parse(&content))
    }
}

#[async_trait]
impl CredentialStore for AgeCredentialStore {
    async fn get(&self, key: &str) -> Result<Option<SecretString>> {
        let field = self.field_name(key);
        let entry = self.read_entry()?;
        Ok(entry.fields.get(field).cloned().map(SecretString::from))
    }

    async fn set(&self, _key: &str, _value: SecretString) -> Result<()> {
        bail!("age credential store is read-only")
    }

    fn supports_write(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn relative_paths_resolve_against_base_dir() {
        let store = AgeCredentialStore::with_base_dir(
            AgeConfig {
                path: "creds/foo.age".to_string(),
                identity_path: Some("key".to_string()),
                fields: HashMap::new(),
            },
            Path::new("/tmp/keepbook"),
        );
        assert_eq!(
            store.resolve_path("creds/foo.age"),
            Path::new("/tmp/keepbook/creds/foo.age")
        );
    }
}
