use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::warn;

use super::Storage;
use crate::credentials::CredentialStore;
use crate::models::{
    Account, AccountConfig, BalanceSnapshot, Connection, ConnectionConfig, ConnectionState, Id,
    Transaction,
};

/// JSON file-based storage implementation.
///
/// Directory structure:
/// ```text
/// data/
///   connections/
///     {id}/
///       connection.toml   # human-declared config
///       connection.json   # machine-managed state
///       accounts/         # symlinks to account dirs
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

    fn connections_by_name_dir(&self) -> PathBuf {
        self.connections_dir().join("by-name")
    }

    fn accounts_dir(&self) -> PathBuf {
        self.base_path.join("accounts")
    }

    fn connection_dir(&self, id: &Id) -> PathBuf {
        self.connections_dir().join(id.to_string())
    }

    fn connection_accounts_dir(&self, id: &Id) -> PathBuf {
        self.connection_dir(id).join("accounts")
    }

    fn connection_config_file(&self, id: &Id) -> PathBuf {
        self.connection_dir(id).join("connection.toml")
    }

    fn connection_state_file(&self, id: &Id) -> PathBuf {
        self.connection_dir(id).join("connection.json")
    }

    /// Get the path to a connection's config file.
    pub fn connection_config_path(&self, id: &Id) -> PathBuf {
        self.connection_config_file(id)
    }

    /// Load the credential store for a connection.
    ///
    /// First checks the connection's config for inline credentials,
    /// then falls back to a separate credentials.toml file for backwards compatibility.
    pub fn get_credential_store(
        &self,
        connection_id: &Id,
    ) -> Result<Option<Box<dyn CredentialStore>>> {
        // First try to load from connection config
        let config_path = self.connection_config_file(connection_id);
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read {}", config_path.display()))?;
            let config: ConnectionConfig = toml::from_str(&content)
                .with_context(|| format!("Failed to parse {}", config_path.display()))?;
            if let Some(cred_config) = config.credentials {
                return Ok(Some(cred_config.build()));
            }
        }

        // Fallback to separate credentials.toml (backwards compatibility)
        let creds_path = self.connection_dir(connection_id).join("credentials.toml");
        if creds_path.exists() {
            let config = crate::credentials::CredentialConfig::load(&creds_path)?;
            return Ok(Some(config.build()));
        }

        Ok(None)
    }

    fn account_dir(&self, id: &Id) -> PathBuf {
        self.accounts_dir().join(id.to_string())
    }

    fn account_file(&self, id: &Id) -> PathBuf {
        self.account_dir(id).join("account.json")
    }

    fn account_config_file(&self, id: &Id) -> PathBuf {
        self.account_dir(id).join("account_config.toml")
    }

    /// Load optional account config.
    fn load_account_config(&self, id: &Id) -> Result<Option<AccountConfig>> {
        let path = self.account_config_file(id);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let config: AccountConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        Ok(Some(config))
    }

    fn balances_file(&self, account_id: &Id) -> PathBuf {
        self.account_dir(account_id).join("balances.jsonl")
    }

    fn transactions_file(&self, account_id: &Id) -> PathBuf {
        self.account_dir(account_id).join("transactions.jsonl")
    }

    /// Sanitize a name for use as a symlink filename.
    /// Returns None if the result would be empty.
    fn sanitize_name(name: &str) -> Option<String> {
        let sanitized: String = name
            .trim()
            .chars()
            .map(|c| if c == '/' || c == '\\' || c == '\0' { '-' } else { c })
            .collect();
        if sanitized.is_empty() {
            None
        } else {
            Some(sanitized)
        }
    }

    async fn ensure_dir(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
        Ok(())
    }

    async fn read_json<T: for<'de> serde::Deserialize<'de>>(
        &self,
        path: &Path,
    ) -> Result<Option<T>> {
        match fs::read_to_string(path).await {
            Ok(content) => {
                let value = serde_json::from_str(&content)
                    .with_context(|| format!("Failed to parse JSON from {path:?}"))?;
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

    fn read_toml_sync<T: for<'de> serde::Deserialize<'de>>(
        &self,
        path: &Path,
    ) -> Result<Option<T>> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let value = toml::from_str(&content)
                    .with_context(|| format!("Failed to parse TOML from {path:?}"))?;
                Ok(Some(value))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("Failed to read file"),
        }
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
                .with_context(|| format!("Failed to parse JSONL line: {line}"))?;
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

    /// Load a connection by reading both config (TOML) and state (JSON).
    async fn load_connection(&self, id: &Id) -> Result<Option<Connection>> {
        let config_path = self.connection_config_file(id);
        let state_path = self.connection_state_file(id);

        // Config is required
        let config: ConnectionConfig = match self.read_toml_sync(&config_path)? {
            Some(c) => c,
            None => return Ok(None),
        };

        // State may not exist yet (new connection)
        let state: ConnectionState = match self.read_json(&state_path).await? {
            Some(s) => s,
            None => {
                // Create default state with the directory name as ID
                ConnectionState {
                    id: id.clone(),
                    ..Default::default()
                }
            }
        };

        Ok(Some(Connection { config, state }))
    }

    /// Collect accounts for a connection, including any that are linked by connection_id
    /// even if the connection state is missing them.
    async fn collect_accounts_for_connection(&self, conn: &Connection) -> Result<Vec<Account>> {
        let mut accounts = Vec::new();
        let mut seen_ids: HashSet<Id> = HashSet::new();

        for account_id in &conn.state.account_ids {
            match self.get_account(account_id).await? {
                Some(account) => {
                    if account.connection_id != *conn.id() {
                        warn!(
                            connection_id = %conn.id(),
                            account_id = %account_id,
                            account_connection_id = %account.connection_id,
                            "account referenced by connection belongs to different connection"
                        );
                        continue;
                    }
                    seen_ids.insert(account.id.clone());
                    accounts.push(account);
                }
                None => {
                    warn!(
                        connection_id = %conn.id(),
                        account_id = %account_id,
                        "account referenced by connection not found"
                    );
                }
            }
        }

        let extra_accounts: Vec<Account> = self
            .list_accounts()
            .await?
            .into_iter()
            .filter(|account| account.connection_id == *conn.id() && !seen_ids.contains(&account.id))
            .collect();

        for account in extra_accounts {
            seen_ids.insert(account.id.clone());
            accounts.push(account);
        }

        Ok(accounts)
    }

    /// Update symlinks from connection's accounts/ dir to the actual account directories.
    ///
    /// Creates symlinks like:
    ///   connections/{conn-id}/accounts/{account-name} -> ../../../accounts/{account-id}
    async fn update_account_symlinks(&self, conn: &Connection) -> Result<usize> {
        let accounts_dir = self.connection_accounts_dir(conn.id());

        // Create or ensure the accounts directory exists
        fs::create_dir_all(&accounts_dir)
            .await
            .context("Failed to create connection accounts directory")?;

        // Remove all existing symlinks
        if let Ok(mut entries) = fs::read_dir(&accounts_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(file_type) = entry.file_type().await {
                    if file_type.is_symlink() {
                        let _ = fs::remove_file(entry.path()).await;
                    }
                }
            }
        }

        // Load accounts and create symlinks by name
        let accounts = self.collect_accounts_for_connection(conn).await?;
        let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut created = 0usize;

        for account in accounts {
            let Some(sanitized) = Self::sanitize_name(&account.name) else {
                warn!(
                    "Skipped account with empty name (id: {}, connection: {})",
                    account.id,
                    conn.id()
                );
                continue;
            };

            if seen_names.contains(&sanitized) {
                warn!(
                    "Skipped duplicate account name \"{}\" (id: {}, connection: {})",
                    sanitized,
                    account.id,
                    conn.id()
                );
                continue;
            }

            let link_path = accounts_dir.join(&sanitized);
            // Relative path: ../../../accounts/{account-id}
            let target = PathBuf::from("../../../accounts").join(account.id.to_string());

            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                // Ignore errors - log them
                if let Err(e) = symlink(&target, &link_path) {
                    warn!(
                        "Failed to create symlink for account \"{}\": {}",
                        sanitized, e
                    );
                    continue;
                }
            }

            seen_names.insert(sanitized);
            created += 1;
        }

        Ok(created)
    }

    /// Rebuild all symlinks in connections/by-name/ directory.
    /// Removes stale symlinks and creates symlinks for all connections by name.
    /// Returns the number of symlinks created and warnings for collisions.
    pub async fn rebuild_connection_symlinks(&self) -> Result<(usize, Vec<String>)> {
        let by_name_dir = self.connections_by_name_dir();

        // Create by-name directory if needed
        fs::create_dir_all(&by_name_dir)
            .await
            .context("Failed to create connections/by-name directory")?;

        // Remove all existing symlinks in by-name/
        let mut entries = match fs::read_dir(&by_name_dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok((0, Vec::new()));
            }
            Err(e) => return Err(e).context("Failed to read by-name directory"),
        };

        while let Some(entry) = entries.next_entry().await? {
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_symlink() {
                    let _ = fs::remove_file(entry.path()).await;
                }
            }
        }

        // Load all connections and create symlinks
        let connections = self.list_connections().await?;
        let mut created = 0;
        let mut warnings = Vec::new();
        let mut seen_names: std::collections::HashMap<String, Id> =
            std::collections::HashMap::new();

        for conn in connections {
            let name = conn.name();
            let Some(sanitized) = Self::sanitize_name(name) else {
                warnings.push(format!(
                    "Skipped connection with empty name (id: {})",
                    conn.id()
                ));
                continue;
            };

            if let Some(existing_id) = seen_names.get(&sanitized) {
                warnings.push(format!(
                    "Skipped duplicate connection name \"{}\" (id: {}, conflicts with {})",
                    sanitized,
                    conn.id(),
                    existing_id
                ));
                continue;
            }

            let link_path = by_name_dir.join(&sanitized);
            let target = PathBuf::from("..").join(conn.id().to_string());

            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                if let Err(e) = symlink(&target, &link_path) {
                    warnings.push(format!("Failed to create symlink for \"{sanitized}\": {e}"));
                    continue;
                }
            }

            seen_names.insert(sanitized, conn.id().clone());
            created += 1;
        }

        Ok((created, warnings))
    }

    /// Rebuild all symlinks: connections/by-name/ and all connection accounts/ directories.
    /// Returns (connections_created, accounts_created, warnings).
    pub async fn rebuild_all_symlinks(&self) -> Result<(usize, usize, Vec<String>)> {
        let (conn_created, mut warnings) = self.rebuild_connection_symlinks().await?;

        let connections = self.list_connections().await?;
        let mut account_created = 0;

        for conn in connections {
            match self.update_account_symlinks(&conn).await {
                Ok(created) => account_created += created,
                Err(e) => warnings.push(format!(
                    "Failed to update account symlinks for connection {}: {}",
                    conn.id(),
                    e
                )),
            }
        }

        Ok((conn_created, account_created, warnings))
    }
}

#[async_trait::async_trait]
impl Storage for JsonFileStorage {
    fn get_credential_store(&self, connection_id: &Id) -> Result<Option<Box<dyn CredentialStore>>> {
        // Delegate to the inherent method
        JsonFileStorage::get_credential_store(self, connection_id)
    }

    fn get_account_config(&self, account_id: &Id) -> Result<Option<AccountConfig>> {
        self.load_account_config(account_id)
    }

    async fn list_connections(&self) -> Result<Vec<Connection>> {
        let ids = self.list_dirs(&self.connections_dir()).await?;
        let mut connections = Vec::new();

        for id in ids {
            if let Some(conn) = self.load_connection(&id).await? {
                connections.push(conn);
            }
        }

        Ok(connections)
    }

    async fn get_connection(&self, id: &Id) -> Result<Option<Connection>> {
        self.load_connection(id).await
    }

    async fn save_connection(&self, conn: &Connection) -> Result<()> {
        // Only save state - config is human-managed
        self.write_json(&self.connection_state_file(conn.id()), &conn.state)
            .await?;
        let _ = self.update_account_symlinks(conn).await?;
        // Rebuild connection by-name symlinks (handles creates and name changes)
        let _ = self.rebuild_connection_symlinks().await;
        Ok(())
    }

    async fn delete_connection(&self, id: &Id) -> Result<bool> {
        let dir = self.connection_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir).await.with_context(|| {
                format!("Failed to delete connection directory: {}", dir.display())
            })?;
            // Rebuild symlinks to remove stale one
            let _ = self.rebuild_connection_symlinks().await;
            Ok(true)
        } else {
            Ok(false)
        }
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
        self.write_json(&self.account_file(&account.id), account)
            .await
    }

    async fn delete_account(&self, id: &Id) -> Result<bool> {
        let dir = self.account_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir).await.with_context(|| {
                format!("Failed to delete account directory: {}", dir.display())
            })?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn get_balance_snapshots(&self, account_id: &Id) -> Result<Vec<BalanceSnapshot>> {
        self.read_jsonl(&self.balances_file(account_id)).await
    }

    async fn append_balance_snapshot(
        &self,
        account_id: &Id,
        snapshot: &BalanceSnapshot,
    ) -> Result<()> {
        self.append_jsonl(&self.balances_file(account_id), &[snapshot])
            .await
    }

    async fn get_transactions(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        self.read_jsonl(&self.transactions_file(account_id)).await
    }

    async fn append_transactions(&self, account_id: &Id, txns: &[Transaction]) -> Result<()> {
        self.append_jsonl(&self.transactions_file(account_id), txns)
            .await
    }

    async fn get_latest_balance_snapshot(
        &self,
        account_id: &Id,
    ) -> Result<Option<BalanceSnapshot>> {
        let snapshots = self.get_balance_snapshots(account_id).await?;
        Ok(snapshots.into_iter().max_by_key(|s| s.timestamp))
    }

    async fn get_latest_balances_for_connection(
        &self,
        connection_id: &Id,
    ) -> Result<Vec<(Id, BalanceSnapshot)>> {
        let connection = self
            .get_connection(connection_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Connection not found"))?;

        let accounts = self.collect_accounts_for_connection(&connection).await?;
        let mut results = Vec::new();

        for account in accounts {
            if let Some(snapshot) = self.get_latest_balance_snapshot(&account.id).await? {
                results.push((account.id.clone(), snapshot));
            }
        }

        Ok(results)
    }

    async fn get_latest_balances(&self) -> Result<Vec<(Id, BalanceSnapshot)>> {
        let connections = self.list_connections().await?;

        let mut results = Vec::new();
        for connection in connections {
            let connection_snapshots = self
                .get_latest_balances_for_connection(connection.id())
                .await?;
            results.extend(connection_snapshots);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::JsonFileStorage;

    #[test]
    fn sanitize_name_replaces_path_separators() {
        assert_eq!(
            JsonFileStorage::sanitize_name("foo/bar"),
            Some("foo-bar".to_string())
        );
        assert_eq!(
            JsonFileStorage::sanitize_name("foo\\bar"),
            Some("foo-bar".to_string())
        );
        assert_eq!(JsonFileStorage::sanitize_name("   "), None);
    }
}
