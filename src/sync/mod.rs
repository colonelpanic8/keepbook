pub mod coinbase;

use anyhow::Result;
use crate::models::{Account, Balance, Connection, Transaction};

/// Result of a sync operation
pub struct SyncResult {
    pub connection: Connection,
    pub accounts: Vec<Account>,
    pub balances: Vec<(uuid::Uuid, Vec<Balance>)>,  // (account_id, balances)
    pub transactions: Vec<(uuid::Uuid, Vec<Transaction>)>,  // (account_id, transactions)
}

/// Trait for synchronizers - fetches data from external sources
///
/// This is intentionally minimal for the proof of concept.
/// We'll learn what abstractions we actually need by building real synchronizers.
#[async_trait::async_trait]
pub trait Synchronizer: Send + Sync {
    /// Human-readable name for this synchronizer
    fn name(&self) -> &str;

    /// Perform a full sync, returning all accounts, balances, and transactions
    async fn sync(&self, connection: &mut Connection) -> Result<SyncResult>;
}
