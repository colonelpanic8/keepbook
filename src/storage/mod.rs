mod json_file;

pub use json_file::JsonFileStorage;

use anyhow::Result;
use uuid::Uuid;
use crate::models::{Account, Balance, Connection, Transaction};

/// Time range for queries
pub struct TimeRange {
    pub start: Option<chrono::DateTime<chrono::Utc>>,
    pub end: Option<chrono::DateTime<chrono::Utc>>,
}

impl TimeRange {
    pub fn all() -> Self {
        Self { start: None, end: None }
    }
}

/// Minimal storage trait - will evolve as we learn what we need
#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    // Connections
    async fn list_connections(&self) -> Result<Vec<Connection>>;
    async fn get_connection(&self, id: &Uuid) -> Result<Option<Connection>>;
    async fn save_connection(&self, conn: &Connection) -> Result<()>;

    // Accounts
    async fn list_accounts(&self) -> Result<Vec<Account>>;
    async fn get_account(&self, id: &Uuid) -> Result<Option<Account>>;
    async fn save_account(&self, account: &Account) -> Result<()>;

    // Balances
    async fn get_balances(&self, account_id: &Uuid, range: &TimeRange) -> Result<Vec<Balance>>;
    async fn append_balances(&self, account_id: &Uuid, balances: &[Balance]) -> Result<()>;

    // Transactions
    async fn get_transactions(&self, account_id: &Uuid, range: &TimeRange) -> Result<Vec<Transaction>>;
    async fn append_transactions(&self, account_id: &Uuid, txns: &[Transaction]) -> Result<()>;
}
