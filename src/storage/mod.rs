mod json_file;
pub mod lookup;
mod memory;

pub use json_file::JsonFileStorage;
pub use lookup::{find_account, find_connection};
pub use memory::MemoryStorage;

use crate::credentials::CredentialStore;
use crate::models::{
    Account, AccountConfig, BalanceSnapshot, Connection, ConnectionConfig, Id, Transaction,
};
use anyhow::Result;

/// Storage trait for persisting financial data.
#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    /// Get the credential store for a connection.
    fn get_credential_store(&self, connection_id: &Id) -> Result<Option<Box<dyn CredentialStore>>>;
    /// Load the optional account config.
    fn get_account_config(&self, account_id: &Id) -> Result<Option<AccountConfig>>;
    // Connections
    async fn list_connections(&self) -> Result<Vec<Connection>>;
    async fn get_connection(&self, id: &Id) -> Result<Option<Connection>>;
    async fn save_connection(&self, conn: &Connection) -> Result<()>;
    async fn delete_connection(&self, id: &Id) -> Result<bool>;
    async fn save_connection_config(&self, id: &Id, config: &ConnectionConfig) -> Result<()>;

    // Accounts
    async fn list_accounts(&self) -> Result<Vec<Account>>;
    async fn get_account(&self, id: &Id) -> Result<Option<Account>>;
    async fn save_account(&self, account: &Account) -> Result<()>;
    async fn delete_account(&self, id: &Id) -> Result<bool>;
    async fn save_account_config(&self, id: &Id, config: &AccountConfig) -> Result<()>;

    // Balance Snapshots
    async fn get_balance_snapshots(&self, account_id: &Id) -> Result<Vec<BalanceSnapshot>>;
    async fn append_balance_snapshot(
        &self,
        account_id: &Id,
        snapshot: &BalanceSnapshot,
    ) -> Result<()>;

    /// Get the most recent balance snapshot for a specific account.
    async fn get_latest_balance_snapshot(&self, account_id: &Id)
        -> Result<Option<BalanceSnapshot>>;

    /// Get the most recent balance snapshot for each account across all accounts.
    async fn get_latest_balances(&self) -> Result<Vec<(Id, BalanceSnapshot)>>;

    /// Get the most recent balance snapshot for each account belonging to a connection.
    async fn get_latest_balances_for_connection(
        &self,
        connection_id: &Id,
    ) -> Result<Vec<(Id, BalanceSnapshot)>>;

    // Transactions
    async fn get_transactions(&self, account_id: &Id) -> Result<Vec<Transaction>>;
    async fn append_transactions(&self, account_id: &Id, txns: &[Transaction]) -> Result<()>;
}
