mod orchestrator;
pub mod schwab;
pub mod synchronizers;

pub use orchestrator::{PriceRefreshResult, SyncOrchestrator};
pub use synchronizers::create_synchronizer;

use crate::market_data::PricePoint;
use crate::models::{Account, AssetBalance, BalanceSnapshot, Connection, Id, Transaction};
use crate::storage::Storage;
use anyhow::Result;

/// An asset balance paired with optional price data from the synchronizer.
#[derive(Debug, Clone)]
pub struct SyncedAssetBalance {
    pub asset_balance: AssetBalance,
    pub price: Option<PricePoint>,
}

impl SyncedAssetBalance {
    pub fn new(asset_balance: AssetBalance) -> Self {
        Self {
            asset_balance,
            price: None,
        }
    }

    pub fn with_price(mut self, price: PricePoint) -> Self {
        self.price = Some(price);
        self
    }
}

/// Result of a sync operation.
pub struct SyncResult {
    pub connection: Connection,
    pub accounts: Vec<Account>,
    pub balances: Vec<(Id, Vec<SyncedAssetBalance>)>,
    pub transactions: Vec<(Id, Vec<Transaction>)>,
}

impl SyncResult {
    /// Save this sync result to storage.
    pub async fn save(&self, storage: &impl Storage) -> Result<()> {
        storage.save_connection(&self.connection).await?;

        for account in &self.accounts {
            storage.save_account(account).await?;
        }

        for (account_id, synced_balances) in &self.balances {
            if !synced_balances.is_empty() {
                let asset_balances: Vec<AssetBalance> = synced_balances
                    .iter()
                    .map(|sb| sb.asset_balance.clone())
                    .collect();
                let snapshot = BalanceSnapshot::now(asset_balances);
                storage
                    .append_balance_snapshot(account_id, &snapshot)
                    .await?;
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

/// Authentication status for synchronizers requiring interactive auth.
#[derive(Debug, Clone)]
pub enum AuthStatus {
    /// Session is valid and can be used
    Valid,
    /// No session exists
    Missing,
    /// Session exists but is expired or invalid
    Expired { reason: String },
}

/// Trait for synchronizers that require interactive (browser-based) authentication.
#[async_trait::async_trait]
pub trait InteractiveAuth: Synchronizer {
    /// Check if the current authentication is valid.
    async fn check_auth(&self) -> Result<AuthStatus>;

    /// Perform interactive login (typically opens a browser).
    async fn login(&mut self) -> Result<()>;
}
