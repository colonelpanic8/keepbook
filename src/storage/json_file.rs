use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::credentials::{CredentialConfig, CredentialStore};
use crate::models::{Account, Balance, Connection, Id, Transaction};
use super::Storage;

/// JSON file-based storage implementation.
///
/// Directory structure:
/// ```text
/// data/
///   connections/
///     {id}/
///       connection.json
///   accounts/
///     {id}/
///       account.json
///       balances.jsonl
///       transactions.jsonl
/// ```
pub struct JsonFileStorage {
    base_path: PathBuf,
}

impl JsonFileStorage {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    fn connections_dir(&self) -> PathBuf {
        self.base_path.join("connections")
    }

    fn accounts_dir(&self) -> PathBuf {
        self.base_path.join("accounts")
    }

    fn connection_dir(&self, id: &Id) -> PathBuf {
        self.connections_dir().join(id.to_string())
    }

    fn connection_file(&self, id: &Id) -> PathBuf {
        self.connection_dir(id).join("connection.json")
    }

    fn credentials_file(&self, id: &Id) -> PathBuf {
        self.connection_dir(id).join("credentials.toml")
    }

    /// Load the credential store for a connection.
    ///
    /// Returns `None` if no `credentials.toml` exists for the connection.
    pub fn get_credential_store(&self, connection_id: &Id) -> Result<Option<Box<dyn CredentialStore>>> {
        let config_path = self.credentials_file(connection_id);
        let config = CredentialConfig::load_optional(&config_path)?;
        Ok(config.map(|c| c.build()))
    }

    /// Get the path to the credentials config file for a connection.
    ///
    /// Useful for creating or editing the credentials configuration.
    pub fn credentials_config_path(&self, connection_id: &Id) -> PathBuf {
        self.credentials_file(connection_id)
    }

    fn account_dir(&self, id: &Id) -> PathBuf {
        self.accounts_dir().join(id.to_string())
    }

    fn account_file(&self, id: &Id) -> PathBuf {
        self.account_dir(id).join("account.json")
    }

    fn balances_file(&self, account_id: &Id) -> PathBuf {
        self.account_dir(account_id).join("balances.jsonl")
    }

    fn transactions_file(&self, account_id: &Id) -> PathBuf {
        self.account_dir(account_id).join("transactions.jsonl")
    }

    async fn ensure_dir(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .context("Failed to create directory")?;
        }
        Ok(())
    }

    async fn read_json<T: for<'de> serde::Deserialize<'de>>(&self, path: &Path) -> Result<Option<T>> {
        match fs::read_to_string(path).await {
            Ok(content) => {
                let value = serde_json::from_str(&content)
                    .with_context(|| format!("Failed to parse JSON from {:?}", path))?;
                Ok(Some(value))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("Failed to read file"),
        }
    }

    async fn write_json<T: serde::Serialize>(&self, path: &Path, value: &T) -> Result<()> {
        self.ensure_dir(path).await?;
        let content = serde_json::to_string_pretty(value).context("Failed to serialize JSON")?;
        fs::write(path, content)
            .await
            .context("Failed to write file")?;
        Ok(())
    }

    async fn read_jsonl<T: for<'de> serde::Deserialize<'de>>(&self, path: &Path) -> Result<Vec<T>> {
        let file = match fs::File::open(path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).context("Failed to open file"),
        };

        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut items = Vec::new();

        while let Some(line) = lines.next_line().await.context("Failed to read line")? {
            if line.trim().is_empty() {
                continue;
            }
            let item: T = serde_json::from_str(&line)
                .with_context(|| format!("Failed to parse JSONL line: {}", line))?;
            items.push(item);
        }

        Ok(items)
    }

    async fn append_jsonl<T: serde::Serialize>(&self, path: &Path, items: &[T]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        self.ensure_dir(path).await?;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .context("Failed to open file for append")?;

        for item in items {
            let line = serde_json::to_string(item).context("Failed to serialize item")?;
            file.write_all(line.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }

        Ok(())
    }

    async fn list_dirs(&self, path: &Path) -> Result<Vec<Id>> {
        let mut ids = Vec::new();

        let mut entries = match fs::read_dir(path).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ids),
            Err(e) => return Err(e).context("Failed to read directory"),
        };

        while let Some(entry) = entries.next_entry().await.context("Failed to read entry")? {
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        if !name.is_empty() {
                            ids.push(Id::from(name));
                        }
                    }
                }
            }
        }

        Ok(ids)
    }
}

#[async_trait::async_trait]
impl Storage for JsonFileStorage {
    async fn list_connections(&self) -> Result<Vec<Connection>> {
        let ids = self.list_dirs(&self.connections_dir()).await?;
        let mut connections = Vec::new();

        for id in ids {
            if let Some(conn) = self.get_connection(&id).await? {
                connections.push(conn);
            }
        }

        Ok(connections)
    }

    async fn get_connection(&self, id: &Id) -> Result<Option<Connection>> {
        self.read_json(&self.connection_file(id)).await
    }

    async fn save_connection(&self, conn: &Connection) -> Result<()> {
        self.write_json(&self.connection_file(&conn.id), conn).await
    }

    async fn list_accounts(&self) -> Result<Vec<Account>> {
        let ids = self.list_dirs(&self.accounts_dir()).await?;
        let mut accounts = Vec::new();

        for id in ids {
            if let Some(account) = self.get_account(&id).await? {
                accounts.push(account);
            }
        }

        Ok(accounts)
    }

    async fn get_account(&self, id: &Id) -> Result<Option<Account>> {
        self.read_json(&self.account_file(id)).await
    }

    async fn save_account(&self, account: &Account) -> Result<()> {
        self.write_json(&self.account_file(&account.id), account).await
    }

    async fn get_balances(&self, account_id: &Id) -> Result<Vec<Balance>> {
        self.read_jsonl(&self.balances_file(account_id)).await
    }

    async fn append_balances(&self, account_id: &Id, balances: &[Balance]) -> Result<()> {
        self.append_jsonl(&self.balances_file(account_id), balances).await
    }

    async fn get_transactions(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        self.read_jsonl(&self.transactions_file(account_id)).await
    }

    async fn append_transactions(&self, account_id: &Id, txns: &[Transaction]) -> Result<()> {
        self.append_jsonl(&self.transactions_file(account_id), txns).await
    }
}
