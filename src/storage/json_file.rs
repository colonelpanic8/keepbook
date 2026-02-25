use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::warn;

use super::{dedupe_transactions_last_write_wins, Storage};
use crate::credentials::CredentialStore;
use crate::models::{
    Account, AccountConfig, BalanceSnapshot, Connection, ConnectionConfig, ConnectionState, Id,
    Transaction, TransactionAnnotation, TransactionAnnotationPatch,
};
use crate::storage::JsonlCompactionStats;

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
///       transaction_annotations.jsonl
/// ```
#[derive(Clone)]
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

    fn ensure_id_path_safe(&self, id: &Id) -> Result<()> {
        let value = id.as_str();
        if Id::is_path_safe(value) {
            Ok(())
        } else {
            anyhow::bail!("Invalid id path segment: {value}");
        }
    }

    fn connection_dir(&self, id: &Id) -> Result<PathBuf> {
        self.ensure_id_path_safe(id)?;
        Ok(self.connections_dir().join(id.to_string()))
    }

    fn connection_accounts_dir(&self, id: &Id) -> Result<PathBuf> {
        Ok(self.connection_dir(id)?.join("accounts"))
    }

    fn connection_config_file(&self, id: &Id) -> Result<PathBuf> {
        Ok(self.connection_dir(id)?.join("connection.toml"))
    }

    fn connection_state_file(&self, id: &Id) -> Result<PathBuf> {
        Ok(self.connection_dir(id)?.join("connection.json"))
    }

    /// Get the path to a connection's config file.
    pub fn connection_config_path(&self, id: &Id) -> Result<PathBuf> {
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
        let config_path = self.connection_config_file(connection_id)?;
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
        let creds_path = self.connection_dir(connection_id)?.join("credentials.toml");
        if creds_path.exists() {
            let config = crate::credentials::CredentialConfig::load(&creds_path)?;
            return Ok(Some(config.build()));
        }

        Ok(None)
    }

    fn account_dir(&self, id: &Id) -> Result<PathBuf> {
        self.ensure_id_path_safe(id)?;
        Ok(self.accounts_dir().join(id.to_string()))
    }

    fn account_file(&self, id: &Id) -> Result<PathBuf> {
        Ok(self.account_dir(id)?.join("account.json"))
    }

    fn account_config_file(&self, id: &Id) -> Result<PathBuf> {
        Ok(self.account_dir(id)?.join("account_config.toml"))
    }

    /// Load optional account config.
    fn load_account_config(&self, id: &Id) -> Result<Option<AccountConfig>> {
        let path = self.account_config_file(id)?;
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let config: AccountConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        Ok(Some(config))
    }

    fn balances_file(&self, account_id: &Id) -> Result<PathBuf> {
        Ok(self.account_dir(account_id)?.join("balances.jsonl"))
    }

    fn transactions_file(&self, account_id: &Id) -> Result<PathBuf> {
        Ok(self.account_dir(account_id)?.join("transactions.jsonl"))
    }

    fn transaction_annotations_file(&self, account_id: &Id) -> Result<PathBuf> {
        Ok(self
            .account_dir(account_id)?
            .join("transaction_annotations.jsonl"))
    }

    /// Sanitize a name for use as a symlink filename.
    /// Returns None if the result would be empty.
    fn sanitize_name(name: &str) -> Option<String> {
        let sanitized: String = name
            .trim()
            .chars()
            .map(|c| {
                if c == '/' || c == '\\' || c == '\0' {
                    '-'
                } else {
                    c
                }
            })
            .collect();
        if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
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

    async fn write_jsonl<T: serde::Serialize>(&self, path: &Path, items: &[T]) -> Result<()> {
        self.ensure_dir(path).await?;

        let mut content = String::new();
        for item in items {
            let line = serde_json::to_string(item).context("Failed to serialize item")?;
            content.push_str(&line);
            content.push('\n');
        }

        fs::write(path, content)
            .await
            .with_context(|| format!("Failed to write JSONL file {}", path.display()))?;
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
                            if Id::is_path_safe(name) {
                                ids.push(Id::from(name));
                            } else {
                                warn!(dir = %path.display(), name = %name, "skipping unsafe id directory");
                            }
                        }
                    }
                }
            }
        }

        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        Ok(ids)
    }

    /// Load a connection by reading both config (TOML) and state (JSON).
    async fn load_connection(&self, id: &Id) -> Result<Option<Connection>> {
        let config_path = self.connection_config_file(id)?;
        let state_path = self.connection_state_file(id)?;

        // Config is required
        let config: ConnectionConfig = match self.read_toml_sync(&config_path)? {
            Some(c) => c,
            None => return Ok(None),
        };

        // State may not exist yet (new connection)
        let mut state: ConnectionState = match self.read_json(&state_path).await? {
            Some(s) => s,
            None => {
                // Create default state with the directory name as ID
                ConnectionState {
                    id: id.clone(),
                    ..Default::default()
                }
            }
        };

        if state.id != *id || !Id::is_path_safe(state.id.as_str()) {
            warn!(
                connection_id = %id,
                state_id = %state.id,
                "connection state id does not match directory id; using directory id"
            );
            state.id = id.clone();
        }

        Ok(Some(Connection { config, state }))
    }

    /// Collect accounts for a connection, including any that are linked by connection_id
    /// even if the connection state is missing them.
    async fn collect_accounts_for_connection(&self, conn: &Connection) -> Result<Vec<Account>> {
        let mut accounts = Vec::new();
        let mut seen_ids: HashSet<Id> = HashSet::new();

        for account_id in &conn.state.account_ids {
            if !Id::is_path_safe(account_id.as_str()) {
                warn!(
                    connection_id = %conn.id(),
                    account_id = %account_id,
                    "skipping account with unsafe id referenced by connection"
                );
                continue;
            }
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
                    if seen_ids.insert(account.id.clone()) {
                        accounts.push(account);
                    }
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
            .filter(|account| {
                account.connection_id == *conn.id() && !seen_ids.contains(&account.id)
            })
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
        let accounts_dir = self.connection_accounts_dir(conn.id())?;

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

            let key = sanitized.to_lowercase();
            if seen_names.contains(&key) {
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
            if !Id::is_path_safe(account.id.as_str()) {
                warn!(
                    "Skipped account with unsafe id \"{}\" (connection: {})",
                    account.id,
                    conn.id()
                );
                continue;
            }
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

            seen_names.insert(key);
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

            let key = sanitized.to_lowercase();
            if let Some(existing_id) = seen_names.get(&key) {
                warnings.push(format!(
                    "Skipped duplicate connection name \"{}\" (id: {}, conflicts with {})",
                    sanitized,
                    conn.id(),
                    existing_id
                ));
                continue;
            }

            if !Id::is_path_safe(conn.id().as_str()) {
                warnings.push(format!(
                    "Skipped connection with unsafe id \"{}\"",
                    conn.id()
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

            seen_names.insert(key, conn.id().clone());
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

    pub async fn recompact_all_jsonl(&self) -> Result<JsonlCompactionStats> {
        let account_ids = self.list_dirs(&self.accounts_dir()).await?;
        let mut stats = JsonlCompactionStats {
            accounts_processed: account_ids.len(),
            ..Default::default()
        };

        for account_id in account_ids {
            let balances_path = self.balances_file(&account_id)?;
            if balances_path.exists() {
                let mut snapshots = self.read_jsonl::<BalanceSnapshot>(&balances_path).await?;
                stats.balance_snapshots_before += snapshots.len();
                snapshots.sort_by_key(|s| s.timestamp);
                stats.balance_snapshots_after += snapshots.len();
                self.write_jsonl(&balances_path, &snapshots).await?;
                stats.files_rewritten += 1;
            }

            let tx_path = self.transactions_file(&account_id)?;
            if tx_path.exists() {
                let raw: Vec<Transaction> = self
                    .read_jsonl::<Transaction>(&tx_path)
                    .await?
                    .into_iter()
                    .map(Transaction::backfill_standardized_metadata)
                    .collect();
                stats.transactions_before += raw.len();
                let mut compacted = dedupe_transactions_last_write_wins(raw);
                compacted.sort_by(|a, b| {
                    a.timestamp
                        .cmp(&b.timestamp)
                        .then_with(|| a.id.as_str().cmp(b.id.as_str()))
                });
                stats.transactions_after += compacted.len();
                self.write_jsonl(&tx_path, &compacted).await?;
                stats.files_rewritten += 1;
            }

            let ann_path = self.transaction_annotations_file(&account_id)?;
            if ann_path.exists() {
                let raw = self
                    .read_jsonl::<TransactionAnnotationPatch>(&ann_path)
                    .await?;
                stats.annotation_patches_before += raw.len();
                let compacted = compact_transaction_annotation_patches(raw);
                stats.annotation_patches_after += compacted.len();
                self.write_jsonl(&ann_path, &compacted).await?;
                stats.files_rewritten += 1;
            }
        }

        Ok(stats)
    }
}

fn compact_transaction_annotation_patches(
    patches: Vec<TransactionAnnotationPatch>,
) -> Vec<TransactionAnnotationPatch> {
    let mut with_index: Vec<(usize, TransactionAnnotationPatch)> =
        patches.into_iter().enumerate().collect();
    with_index.sort_by_key(|(_, p)| p.timestamp);

    let mut by_tx: std::collections::HashMap<
        Id,
        (TransactionAnnotation, chrono::DateTime<chrono::Utc>),
    > = std::collections::HashMap::new();

    for (_, patch) in with_index {
        let tx_id = patch.transaction_id.clone();
        let entry = by_tx
            .entry(tx_id.clone())
            .or_insert_with(|| (TransactionAnnotation::new(tx_id.clone()), patch.timestamp));
        patch.apply_to(&mut entry.0);
        entry.1 = patch.timestamp;
    }

    let mut out = Vec::new();
    for (tx_id, (ann, timestamp)) in by_tx {
        if ann.is_empty() {
            continue;
        }
        out.push(TransactionAnnotationPatch {
            transaction_id: tx_id,
            timestamp,
            description: ann.description.map(Some),
            note: ann.note.map(Some),
            category: ann.category.map(Some),
            tags: ann.tags.map(Some),
        });
    }

    out.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.transaction_id.as_str().cmp(b.transaction_id.as_str()))
    });
    out
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
            match self.load_connection(&id).await {
                Ok(Some(conn)) => connections.push(conn),
                Ok(None) => {}
                Err(err) => {
                    warn!(
                        connection_id = %id,
                        error = %err,
                        "skipping connection with invalid config/state"
                    );
                }
            }
        }

        Ok(connections)
    }

    async fn get_connection(&self, id: &Id) -> Result<Option<Connection>> {
        self.load_connection(id).await
    }

    async fn save_connection(&self, conn: &Connection) -> Result<()> {
        // Only save state - config is human-managed
        let state_path = self.connection_state_file(conn.id())?;
        self.write_json(&state_path, &conn.state).await?;
        let _ = self.update_account_symlinks(conn).await?;
        // Rebuild connection by-name symlinks (handles creates and name changes)
        let _ = self.rebuild_connection_symlinks().await;
        Ok(())
    }

    async fn delete_connection(&self, id: &Id) -> Result<bool> {
        let dir = self.connection_dir(id)?;
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

    async fn save_connection_config(&self, id: &Id, config: &ConnectionConfig) -> Result<()> {
        let path = self.connection_config_file(id)?;
        self.ensure_dir(&path).await?;
        let config_toml =
            toml::to_string_pretty(config).context("Failed to serialize connection config")?;
        fs::write(&path, config_toml)
            .await
            .with_context(|| format!("Failed to write {}", path.display()))?;
        Ok(())
    }

    async fn list_accounts(&self) -> Result<Vec<Account>> {
        let ids = self.list_dirs(&self.accounts_dir()).await?;
        let mut accounts = Vec::new();

        for id in ids {
            match self.get_account(&id).await {
                Ok(Some(account)) => accounts.push(account),
                Ok(None) => {}
                Err(err) => {
                    warn!(
                        account_id = %id,
                        error = %err,
                        "skipping account with invalid json"
                    );
                }
            }
        }

        Ok(accounts)
    }

    async fn get_account(&self, id: &Id) -> Result<Option<Account>> {
        let path = self.account_file(id)?;
        let mut account: Account = match self.read_json(&path).await? {
            Some(account) => account,
            None => return Ok(None),
        };

        if account.id != *id || !Id::is_path_safe(account.id.as_str()) {
            warn!(
                account_id = %id,
                stored_id = %account.id,
                "account id does not match directory id; using directory id"
            );
            account.id = id.clone();
        }

        Ok(Some(account))
    }

    async fn save_account(&self, account: &Account) -> Result<()> {
        let path = self.account_file(&account.id)?;
        self.write_json(&path, account).await
    }

    async fn save_account_config(&self, id: &Id, config: &AccountConfig) -> Result<()> {
        let path = self.account_config_file(id)?;
        self.ensure_dir(&path).await?;
        let config_toml =
            toml::to_string_pretty(config).context("Failed to serialize account config")?;
        fs::write(&path, config_toml)
            .await
            .with_context(|| format!("Failed to write {}", path.display()))?;
        Ok(())
    }

    async fn delete_account(&self, id: &Id) -> Result<bool> {
        let dir = self.account_dir(id)?;
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
        let path = self.balances_file(account_id)?;
        self.read_jsonl(&path).await
    }

    async fn append_balance_snapshot(
        &self,
        account_id: &Id,
        snapshot: &BalanceSnapshot,
    ) -> Result<()> {
        let path = self.balances_file(account_id)?;
        self.append_jsonl(&path, &[snapshot]).await
    }

    async fn get_transactions(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        let txns = self.get_transactions_raw(account_id).await?;
        Ok(dedupe_transactions_last_write_wins(txns))
    }

    async fn get_transactions_raw(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        let path = self.transactions_file(account_id)?;
        Ok(self
            .read_jsonl::<Transaction>(&path)
            .await?
            .into_iter()
            .map(Transaction::backfill_standardized_metadata)
            .collect())
    }

    async fn append_transactions(&self, account_id: &Id, txns: &[Transaction]) -> Result<()> {
        let path = self.transactions_file(account_id)?;
        self.append_jsonl(&path, txns).await
    }

    async fn get_transaction_annotation_patches(
        &self,
        account_id: &Id,
    ) -> Result<Vec<crate::models::TransactionAnnotationPatch>> {
        let path = self.transaction_annotations_file(account_id)?;
        self.read_jsonl(&path).await
    }

    async fn append_transaction_annotation_patches(
        &self,
        account_id: &Id,
        patches: &[crate::models::TransactionAnnotationPatch],
    ) -> Result<()> {
        let path = self.transaction_annotations_file(account_id)?;
        self.append_jsonl(&path, patches).await
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
    use chrono::{TimeZone, Utc};

    use crate::models::{
        Account, Asset, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig,
        ConnectionState, Id, Transaction, TransactionAnnotationPatch,
    };
    use crate::storage::Storage;

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
        assert_eq!(JsonFileStorage::sanitize_name("."), None);
        assert_eq!(JsonFileStorage::sanitize_name(".."), None);
    }

    #[tokio::test]
    async fn list_accounts_returns_ids_in_sorted_order() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let storage = JsonFileStorage::new(temp.path());
        let created_at = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let connection_id = Id::from_string("conn-1");

        storage
            .save_account(&Account::new_with(
                Id::from_string("acct-b"),
                created_at,
                "B",
                connection_id.clone(),
            ))
            .await?;
        storage
            .save_account(&Account::new_with(
                Id::from_string("acct-a"),
                created_at,
                "A",
                connection_id,
            ))
            .await?;

        let ids: Vec<String> = storage
            .list_accounts()
            .await?
            .into_iter()
            .map(|a| a.id.to_string())
            .collect();
        assert_eq!(ids, vec!["acct-a".to_string(), "acct-b".to_string()]);
        Ok(())
    }

    #[tokio::test]
    async fn list_connections_returns_ids_in_sorted_order() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let storage = JsonFileStorage::new(temp.path());
        let created_at = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        let conn_b = Connection {
            config: ConnectionConfig {
                name: "B".to_string(),
                synchronizer: "manual".to_string(),
                credentials: None,
                balance_staleness: None,
            },
            state: ConnectionState::new_with(Id::from_string("conn-b"), created_at),
        };
        let conn_a = Connection {
            config: ConnectionConfig {
                name: "A".to_string(),
                synchronizer: "manual".to_string(),
                credentials: None,
                balance_staleness: None,
            },
            state: ConnectionState::new_with(Id::from_string("conn-a"), created_at),
        };

        storage
            .save_connection_config(conn_b.id(), &conn_b.config)
            .await?;
        storage.save_connection(&conn_b).await?;
        storage
            .save_connection_config(conn_a.id(), &conn_a.config)
            .await?;
        storage.save_connection(&conn_a).await?;

        let ids: Vec<String> = storage
            .list_connections()
            .await?
            .into_iter()
            .map(|c| c.id().to_string())
            .collect();
        assert_eq!(ids, vec!["conn-a".to_string(), "conn-b".to_string()]);
        Ok(())
    }

    #[tokio::test]
    async fn recompact_all_jsonl_compacts_and_sorts_logs() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let storage = JsonFileStorage::new(temp.path());
        let created_at = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let connection_id = Id::from_string("conn-1");
        let account_id = Id::from_string("acct-1");

        storage
            .save_account(&Account::new_with(
                account_id.clone(),
                created_at,
                "Checking",
                connection_id,
            ))
            .await?;

        let older_snapshot = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::currency("USD"), "10.0")],
        );
        let newer_snapshot = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2024, 6, 1, 0, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::currency("USD"), "20.0")],
        );
        storage
            .append_balance_snapshot(&account_id, &newer_snapshot)
            .await?;
        storage
            .append_balance_snapshot(&account_id, &older_snapshot)
            .await?;

        let tx_old = Transaction::new("-10.0", Asset::currency("USD"), "old")
            .with_id(Id::from_string("tx-1"))
            .with_timestamp(Utc.with_ymd_and_hms(2024, 2, 1, 10, 0, 0).unwrap());
        let tx_new = Transaction::new("-12.0", Asset::currency("USD"), "new")
            .with_id(Id::from_string("tx-1"))
            .with_timestamp(Utc.with_ymd_and_hms(2024, 2, 2, 10, 0, 0).unwrap());
        let tx_other = Transaction::new("5.0", Asset::currency("USD"), "credit")
            .with_id(Id::from_string("tx-2"))
            .with_timestamp(Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap());
        storage
            .append_transactions(&account_id, &[tx_old, tx_new, tx_other])
            .await?;

        let patch_note = TransactionAnnotationPatch {
            transaction_id: Id::from_string("tx-anno"),
            timestamp: Utc.with_ymd_and_hms(2024, 2, 3, 12, 0, 0).unwrap(),
            description: None,
            note: Some(Some("memo".to_string())),
            category: None,
            tags: None,
        };
        let patch_category = TransactionAnnotationPatch {
            transaction_id: Id::from_string("tx-anno"),
            timestamp: Utc.with_ymd_and_hms(2024, 2, 4, 12, 0, 0).unwrap(),
            description: None,
            note: None,
            category: Some(Some("food".to_string())),
            tags: None,
        };
        let patch_set_then_clear_a = TransactionAnnotationPatch {
            transaction_id: Id::from_string("tx-clear"),
            timestamp: Utc.with_ymd_and_hms(2024, 2, 5, 12, 0, 0).unwrap(),
            description: Some(Some("temp".to_string())),
            note: None,
            category: None,
            tags: None,
        };
        let patch_set_then_clear_b = TransactionAnnotationPatch {
            transaction_id: Id::from_string("tx-clear"),
            timestamp: Utc.with_ymd_and_hms(2024, 2, 6, 12, 0, 0).unwrap(),
            description: Some(None),
            note: None,
            category: None,
            tags: None,
        };
        storage
            .append_transaction_annotation_patches(
                &account_id,
                &[
                    patch_category,
                    patch_note,
                    patch_set_then_clear_a,
                    patch_set_then_clear_b,
                ],
            )
            .await?;

        let stats = storage.recompact_all_jsonl().await?;
        assert_eq!(stats.accounts_processed, 1);
        assert_eq!(stats.files_rewritten, 3);
        assert_eq!(stats.balance_snapshots_before, 2);
        assert_eq!(stats.balance_snapshots_after, 2);
        assert_eq!(stats.transactions_before, 3);
        assert_eq!(stats.transactions_after, 2);
        assert_eq!(stats.annotation_patches_before, 4);
        assert_eq!(stats.annotation_patches_after, 1);

        let snapshots = storage.get_balance_snapshots(&account_id).await?;
        assert_eq!(snapshots.len(), 2);
        assert!(snapshots[0].timestamp < snapshots[1].timestamp);

        let tx_raw = storage.get_transactions_raw(&account_id).await?;
        assert_eq!(tx_raw.len(), 2);
        assert!(tx_raw[0].timestamp <= tx_raw[1].timestamp);
        assert_eq!(tx_raw[0].id.as_str(), "tx-2");
        assert_eq!(tx_raw[1].id.as_str(), "tx-1");
        assert_eq!(tx_raw[1].description, "new");

        let patches = storage
            .get_transaction_annotation_patches(&account_id)
            .await?;
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].transaction_id.as_str(), "tx-anno");
        assert_eq!(
            patches[0].note.as_ref().cloned().flatten(),
            Some("memo".to_string())
        );
        assert_eq!(
            patches[0].category.as_ref().cloned().flatten(),
            Some("food".to_string())
        );

        Ok(())
    }
}
