use std::collections::HashMap;
use std::io::BufReader;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use super::field_entry::FieldEntry;
use super::CredentialStore;

const AGE_IDENTITY_PATH_ENV: &str = "KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH";
const DEFAULT_SSH_IDENTITY_FILES: &[&str] = &[
    "id_ed25519",
    "id_rsa",
    "id_ecdsa",
    "id_ecdsa_sk",
    "id_ed25519_sk",
];

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

    fn configured_identity_path(&self) -> Result<Option<PathBuf>> {
        if let Some(path) = self
            .config
            .identity_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            return Ok(Some(self.resolve_path(path)));
        }

        match std::env::var(AGE_IDENTITY_PATH_ENV) {
            Ok(path) if !path.trim().is_empty() => Ok(Some(self.resolve_path(path.trim()))),
            Ok(_) | Err(std::env::VarError::NotPresent) => Ok(None),
            Err(err) => Err(err).with_context(|| format!("Failed to read {AGE_IDENTITY_PATH_ENV}")),
        }
    }

    fn default_identity_paths() -> Vec<PathBuf> {
        let Some(home_dir) = dirs::home_dir() else {
            return Vec::new();
        };
        let ssh_dir = home_dir.join(".ssh");
        DEFAULT_SSH_IDENTITY_FILES
            .iter()
            .map(|name| ssh_dir.join(name))
            .filter(|path| path.exists())
            .collect()
    }

    fn identity_paths(&self) -> Result<Vec<PathBuf>> {
        if let Some(path) = self.configured_identity_path()? {
            return Ok(vec![path]);
        }

        let paths = Self::default_identity_paths();
        if paths.is_empty() {
            bail!(
                "age credential identity is not configured; set identity_path or {AGE_IDENTITY_PATH_ENV}, or create a default SSH identity under ~/.ssh"
            );
        }
        Ok(paths)
    }

    fn decrypt_with_identity(&self, ciphertext: &[u8], identity_path: &PathBuf) -> Result<Vec<u8>> {
        let identity_pem = std::fs::read_to_string(identity_path).with_context(|| {
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

        ::age::decrypt(&identity, ciphertext)
            .with_context(|| format!("Failed to decrypt with {}", identity_path.display()))
    }

    fn read_entry(&self) -> Result<FieldEntry> {
        let age_path = self.resolve_path(&self.config.path);
        let identity_paths = self.identity_paths()?;
        let ciphertext = std::fs::read(&age_path)
            .with_context(|| format!("Failed to read age credentials {}", age_path.display()))?;
        let mut failures = Vec::new();
        let mut plaintext = None;
        for identity_path in &identity_paths {
            match self.decrypt_with_identity(&ciphertext, identity_path) {
                Ok(decrypted) => {
                    plaintext = Some(decrypted);
                    break;
                }
                Err(err) => failures.push(format!("{}: {err:#}", identity_path.display())),
            }
        }
        let plaintext = plaintext.ok_or_else(|| {
            anyhow!(
                "Failed to decrypt {} with configured/default SSH identities: {}",
                age_path.display(),
                failures.join("; ")
            )
        })?;
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

    #[test]
    fn configured_identity_path_wins() -> Result<()> {
        let store = AgeCredentialStore::with_base_dir(
            AgeConfig {
                path: "creds/foo.age".to_string(),
                identity_path: Some("key".to_string()),
                fields: HashMap::new(),
            },
            Path::new("/tmp/keepbook"),
        );
        assert_eq!(
            store.configured_identity_path()?,
            Some(Path::new("/tmp/keepbook/key").to_path_buf())
        );
        Ok(())
    }
}
