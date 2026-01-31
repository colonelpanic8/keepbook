use anyhow::Result;
use crate::models::{Account, Balance, Connection, Id, Transaction};
use crate::storage::Storage;

/// Result of a sync operation.
pub struct SyncResult {
    pub connection: Connection,
    pub accounts: Vec<Account>,
    pub balances: Vec<(Id, Vec<Balance>)>,
    pub transactions: Vec<(Id, Vec<Transaction>)>,
}

impl SyncResult {
    /// Save this sync result to storage.
    pub async fn save(&self, storage: &impl Storage) -> Result<()> {
        storage.save_connection(&self.connection).await?;

        for account in &self.accounts {
            storage.save_account(account).await?;
        }

        for (account_id, balances) in &self.balances {
            if !balances.is_empty() {
                storage.append_balances(account_id, balances).await?;
            }
        }

        for (account_id, txns) in &self.transactions {
            if !txns.is_empty() {
                storage.append_transactions(account_id, txns).await?;
            }
        }

        Ok(())
    }
}

/// Trait for synchronizers - fetches data from external sources.
///
/// This is intentionally minimal. We'll learn what abstractions we
/// actually need by building real synchronizers.
#[async_trait::async_trait]
pub trait Synchronizer: Send + Sync {
    /// Human-readable name for this synchronizer
    fn name(&self) -> &str;

    /// Perform a full sync, returning all accounts, balances, and transactions
    async fn sync(&self, connection: &mut Connection) -> Result<SyncResult>;
}
