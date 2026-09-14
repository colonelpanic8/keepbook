pub mod chase;
mod factory;
mod orchestrator;
mod prices;
pub mod schwab;
mod service;
pub mod synchronizers;

pub use factory::{create_synchronizer, DefaultSynchronizerFactory, SynchronizerFactory};
pub use orchestrator::{PriceRefreshResult, SyncOrchestrator, SyncWithPricesResult};
pub use prices::store_sync_prices;
pub use service::{
    AuthPrompter, AutoCommitter, FixedAuthPrompter, GitAutoCommitter, NoopAutoCommitter,
    SyncContext, SyncOutcome, SyncService,
};

use crate::clock::{Clock, SystemClock};
use crate::market_data::PricePoint;
use crate::models::{Account, AssetBalance, BalanceSnapshot, Connection, Id, Transaction};
use crate::storage::Storage;
use anyhow::Result;
use std::collections::HashSet;

/// How to sync transactions for a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransactionSyncMode {
    /// Default behavior: synchronizer may stop early when it detects overlap with stored history.
    #[default]
    Auto,
    /// Backfill as far back as the synchronizer/provider can fetch (bounded by safety limits).
    Full,
}

/// Options that control how a sync is performed.
#[derive(Debug, Clone)]
pub struct SyncOptions {
    pub transactions: TransactionSyncMode,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            transactions: TransactionSyncMode::Auto,
        }
    }
}

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

/// What a synchronizer learned about one account's balances.
///
/// An empty balance list is ambiguous on its own: it can mean the account holds
/// nothing, or that the request for its holdings failed. Recording the first as
/// a zero balance would erase real holdings after a transient error, so the two
/// are distinct cases here and only a snapshot is ever written.
#[derive(Debug, Clone)]
pub enum AccountBalances {
    /// Everything the account holds at sync time. An empty snapshot means the
    /// account holds nothing and is recorded as such.
    Snapshot(Vec<SyncedAssetBalance>),
    /// Holdings could not be retrieved. Stored history is left untouched.
    Unavailable { reason: String },
}

impl AccountBalances {
    pub fn snapshot(balances: Vec<SyncedAssetBalance>) -> Self {
        Self::Snapshot(balances)
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    /// A complete snapshot, or `Unavailable` when the list is empty.
    ///
    /// For adapters that cannot yet tell "holds nothing" apart from "the
    /// request came back without a figure", which is the safe reading.
    pub fn snapshot_or_unavailable(
        balances: Vec<SyncedAssetBalance>,
        reason: impl Into<String>,
    ) -> Self {
        if balances.is_empty() {
            Self::unavailable(reason)
        } else {
            Self::Snapshot(balances)
        }
    }

    /// The retrieved balances, or an empty slice when they are unavailable.
    pub fn balances(&self) -> &[SyncedAssetBalance] {
        match self {
            Self::Snapshot(balances) => balances,
            Self::Unavailable { .. } => &[],
        }
    }

    pub fn unavailable_reason(&self) -> Option<&str> {
        match self {
            Self::Snapshot(_) => None,
            Self::Unavailable { reason } => Some(reason),
        }
    }
}

/// Whether a sync enumerated every account a connection has.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AccountListing {
    /// Every account the connection holds is present in the result. Stored
    /// accounts missing from it have genuinely gone away.
    #[default]
    Complete,
    /// Only some accounts could be listed. Missing accounts say nothing about
    /// whether they still exist, so none are deactivated.
    Partial { reason: String },
}

impl AccountListing {
    pub fn partial(reason: impl Into<String>) -> Self {
        Self::Partial {
            reason: reason.into(),
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }
}

/// Result of a sync operation.
#[derive(Debug)]
pub struct SyncResult {
    pub connection: Connection,
    pub accounts: Vec<Account>,
    /// Whether `accounts` is the connection's full account list. Accounts
    /// stored but missing from a complete listing are deactivated.
    pub account_listing: AccountListing,
    pub balances: Vec<(Id, AccountBalances)>,
    pub transactions: Vec<(Id, Vec<Transaction>)>,
}

impl SyncResult {
    /// Save this sync result to storage.
    pub async fn save(&self, storage: &dyn Storage) -> Result<()> {
        self.save_with_clock(storage, &SystemClock).await
    }

    pub async fn save_with_clock(&self, storage: &dyn Storage, clock: &dyn Clock) -> Result<()> {
        let synced_account_ids: HashSet<Id> = self
            .accounts
            .iter()
            .map(|account| account.id.clone())
            .collect();

        for account in &self.accounts {
            storage.save_account(account).await?;
        }

        // Only a complete listing proves a stored account is gone. After a
        // partial listing, an absent account may simply not have been reached.
        if self.account_listing.is_complete() {
            for account in storage.list_accounts().await? {
                if account.connection_id == *self.connection.id()
                    && account.active
                    && !synced_account_ids.contains(&account.id)
                {
                    let mut inactive_account = account.clone();
                    inactive_account.active = false;
                    storage.save_account(&inactive_account).await?;

                    let snapshot = BalanceSnapshot::now_with(clock, Vec::new());
                    storage
                        .append_balance_snapshot(&inactive_account.id, &snapshot)
                        .await?;
                }
            }
        }

        for (account_id, account_balances) in &self.balances {
            match account_balances {
                AccountBalances::Snapshot(synced_balances) => {
                    let asset_balances: Vec<AssetBalance> = synced_balances
                        .iter()
                        .map(|sb| sb.asset_balance.clone())
                        .collect();
                    let snapshot = BalanceSnapshot::now_with(clock, asset_balances);
                    storage
                        .append_balance_snapshot(account_id, &snapshot)
                        .await?;
                }
                AccountBalances::Unavailable { reason } => {
                    tracing::warn!(
                        account_id = %account_id,
                        reason = %reason,
                        "balances unavailable; leaving stored history unchanged"
                    );
                }
            }
        }

        for (account_id, txns) in &self.transactions {
            if !txns.is_empty() {
                // Be (mostly) idempotent while still allowing "updates" to existing transactions
                // (e.g. pending -> posted). We treat transaction id as the stable identity and
                // only append a new version if something actually changed.
                //
                // This is intentionally storage-agnostic. If this becomes slow for large histories,
                // we can push an indexed/streaming implementation into Storage.
                let existing = storage.get_transactions(account_id).await?;
                let existing_by_id: std::collections::HashMap<Id, Transaction> =
                    existing.into_iter().map(|t| (t.id.clone(), t)).collect();

                // Collapse duplicates within this batch: last write wins, preserve first-seen order.
                let mut candidate_txns: Vec<Transaction> = Vec::new();
                let mut idx_by_id: std::collections::HashMap<Id, usize> =
                    std::collections::HashMap::new();
                for txn in txns {
                    if let Some(idx) = idx_by_id.get(&txn.id).copied() {
                        candidate_txns[idx] = txn.clone();
                    } else {
                        idx_by_id.insert(txn.id.clone(), candidate_txns.len());
                        candidate_txns.push(txn.clone());
                    }
                }

                let mut to_append: Vec<Transaction> = Vec::new();
                for txn in candidate_txns {
                    if let Some(existing) = existing_by_id.get(&txn.id) {
                        let unchanged = existing.timestamp == txn.timestamp
                            && existing.amount == txn.amount
                            && existing.asset == txn.asset
                            && existing.description == txn.description
                            && existing.status == txn.status
                            && existing.synchronizer_data == txn.synchronizer_data
                            && existing.standardized_metadata == txn.standardized_metadata;
                        if unchanged {
                            continue;
                        }
                    }
                    to_append.push(txn);
                }

                if !to_append.is_empty() {
                    storage.append_transactions(account_id, &to_append).await?;
                }
            }
        }

        // Advance sync cursors only after the data they acknowledge has been saved.
        storage.save_connection(&self.connection).await?;
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

    /// Perform a full sync, returning all accounts, balances, and transactions.
    async fn sync(&self, connection: &mut Connection, storage: &dyn Storage) -> Result<SyncResult>;

    /// Perform a sync with options.
    ///
    /// Default implementation ignores options and calls `sync`.
    async fn sync_with_options(
        &self,
        connection: &mut Connection,
        storage: &dyn Storage,
        options: &SyncOptions,
    ) -> Result<SyncResult> {
        let _ = options;
        self.sync(connection, storage).await
    }

    /// Return interactive auth support if this synchronizer needs it.
    fn interactive(&mut self) -> Option<&mut dyn InteractiveAuth> {
        None
    }
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
    /// Whether auth is required before running `sync`.
    ///
    /// Some synchronizers (e.g. Chase) can proceed without a cached session because the
    /// sync itself is interactive and the user will log in during the flow.
    fn auth_required_for_sync(&self) -> bool {
        true
    }

    /// Check if the current authentication is valid.
    async fn check_auth(&self) -> Result<AuthStatus>;

    /// Perform interactive login (typically opens a browser).
    async fn login(&mut self) -> Result<()>;
}
