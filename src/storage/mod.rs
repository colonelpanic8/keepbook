mod json_file;

pub use json_file::JsonFileStorage;

use anyhow::Result;
use crate::models::{Account, Balance, Connection, Id, Transaction};

/// Storage trait for persisting financial data.
#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    // Connections
    async fn list_connections(&self) -> Result<Vec<Connection>>;
    async fn get_connection(&self, id: &Id) -> Result<Option<Connection>>;
    async fn save_connection(&self, conn: &Connection) -> Result<()>;

    // Accounts
    async fn list_accounts(&self) -> Result<Vec<Account>>;
    async fn get_account(&self, id: &Id) -> Result<Option<Account>>;
    async fn save_account(&self, account: &Account) -> Result<()>;

    // Balances
    async fn get_balances(&self, account_id: &Id) -> Result<Vec<Balance>>;
    async fn append_balances(&self, account_id: &Id, balances: &[Balance]) -> Result<()>;

    /// Get the most recent balance for each (account, asset) pair across all accounts.
    async fn get_latest_balances(&self) -> Result<Vec<(Id, Balance)>>;

    /// Get the most recent balance for each asset in accounts belonging to a connection.
    async fn get_latest_balances_for_connection(&self, connection_id: &Id) -> Result<Vec<(Id, Balance)>>;

    /// Get the most recent balance for each asset in a specific account.
    async fn get_latest_balances_for_account(&self, account_id: &Id) -> Result<Vec<Balance>>;

    // Transactions
    async fn get_transactions(&self, account_id: &Id) -> Result<Vec<Transaction>>;
    async fn append_transactions(&self, account_id: &Id, txns: &[Transaction]) -> Result<()>;
}
