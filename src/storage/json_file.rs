use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use chrono::{Datelike, Utc};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

use crate::models::{Account, Balance, Connection, Transaction};
use super::{Storage, TimeRange};

/// JSON file-based storage implementation
///
/// Directory structure:
/// ```
/// data/
///   connections/
///     {uuid}/
///       connection.json
///   accounts/
///     {uuid}/
///       account.json
///       2024/
///         01-balances.jsonl
///         01-transactions.jsonl
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

    fn connection_file(&self, id: &Uuid) -> PathBuf {
        self.connections_dir().join(id.to_string()).join("connection.json")
    }

    fn account_file(&self, id: &Uuid) -> PathBuf {
        self.accounts_dir().join(id.to_string()).join("account.json")
    }

    fn account_year_dir(&self, account_id: &Uuid, year: i32) -> PathBuf {
        self.accounts_dir()
            .join(account_id.to_string())
            .join(year.to_string())
    }

    fn balances_file(&self, account_id: &Uuid, year: i32, month: u32) -> PathBuf {
        self.account_year_dir(account_id, year)
            .join(format!("{:02}-balances.jsonl", month))
    }

    fn transactions_file(&self, account_id: &Uuid, year: i32, month: u32) -> PathBuf {
        self.account_year_dir(account_id, year)
            .join(format!("{:02}-transactions.jsonl", month))
    }

    async fn ensure_dir(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await
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
        let content = serde_json::to_string_pretty(value)
            .context("Failed to serialize JSON")?;
        fs::write(path, content).await
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
            let line = serde_json::to_string(item)
                .context("Failed to serialize item")?;
            file.write_all(line.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }

        Ok(())
    }

    async fn list_dirs(&self, path: &Path) -> Result<Vec<Uuid>> {
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
                        if let Ok(id) = Uuid::parse_str(name) {
                            ids.push(id);
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

    async fn get_connection(&self, id: &Uuid) -> Result<Option<Connection>> {
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

    async fn get_account(&self, id: &Uuid) -> Result<Option<Account>> {
        self.read_json(&self.account_file(id)).await
    }

    async fn save_account(&self, account: &Account) -> Result<()> {
        self.write_json(&self.account_file(&account.id), account).await
    }

    async fn get_balances(&self, account_id: &Uuid, range: &TimeRange) -> Result<Vec<Balance>> {
        // For now, just get current year/month. In a real impl, would iterate over range.
        let now = Utc::now();
        let path = self.balances_file(account_id, now.year(), now.month());
        let mut balances: Vec<Balance> = self.read_jsonl(&path).await?;

        // Filter by range if specified
        if let Some(start) = range.start {
            balances.retain(|b| b.timestamp >= start);
        }
        if let Some(end) = range.end {
            balances.retain(|b| b.timestamp <= end);
        }

        Ok(balances)
    }

    async fn append_balances(&self, account_id: &Uuid, balances: &[Balance]) -> Result<()> {
        // Group balances by year/month and append to appropriate files
        use std::collections::HashMap;

        let mut grouped: HashMap<(i32, u32), Vec<&Balance>> = HashMap::new();

        for balance in balances {
            let key = (balance.timestamp.year(), balance.timestamp.month());
            grouped.entry(key).or_default().push(balance);
        }

        for ((year, month), items) in grouped {
            let path = self.balances_file(account_id, year, month);
            self.append_jsonl(&path, &items).await?;
        }

        Ok(())
    }

    async fn get_transactions(&self, account_id: &Uuid, range: &TimeRange) -> Result<Vec<Transaction>> {
        // For now, just get current year/month. In a real impl, would iterate over range.
        let now = Utc::now();
        let path = self.transactions_file(account_id, now.year(), now.month());
        let mut transactions: Vec<Transaction> = self.read_jsonl(&path).await?;

        // Filter by range if specified
        if let Some(start) = range.start {
            transactions.retain(|t| t.timestamp >= start);
        }
        if let Some(end) = range.end {
            transactions.retain(|t| t.timestamp <= end);
        }

        Ok(transactions)
    }

    async fn append_transactions(&self, account_id: &Uuid, txns: &[Transaction]) -> Result<()> {
        // Group transactions by year/month and append to appropriate files
        use std::collections::HashMap;

        let mut grouped: HashMap<(i32, u32), Vec<&Transaction>> = HashMap::new();

        for txn in txns {
            let key = (txn.timestamp.year(), txn.timestamp.month());
            grouped.entry(key).or_default().push(txn);
        }

        for ((year, month), items) in grouped {
            let path = self.transactions_file(account_id, year, month);
            self.append_jsonl(&path, &items).await?;
        }

        Ok(())
    }
}
