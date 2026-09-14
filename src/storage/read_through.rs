//! Request-scoped memoization for storage reads.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;

use crate::credentials::CredentialStore;
use crate::models::{
    Account, AccountConfig, BalanceSnapshot, Connection, ConnectionConfig, Id,
    ProposedTransactionEdit, RecurringTransactionReview, Transaction, TransactionAnnotationPatch,
};

use super::Storage;

#[derive(Default)]
struct ReadThroughCache {
    accounts: Option<Vec<Account>>,
    connections: Option<Vec<Connection>>,
    account_configs: HashMap<Id, Option<AccountConfig>>,
    balance_snapshots: HashMap<Id, Vec<BalanceSnapshot>>,
    transactions: HashMap<Id, Vec<Transaction>>,
}

/// Reads accounts, connections, configs, balances, and transactions at most
/// once each.
///
/// Valuing a series of history points reloads the same account and balance data
/// per point. Wrap storage in this for the duration of one such request. Writes
/// pass through and clear what was memoized, and the wrapper is dropped when the
/// request ends, so there is no cache to invalidate across requests.
pub struct ReadThroughStorage {
    inner: Arc<dyn Storage>,
    cache: Mutex<ReadThroughCache>,
}

impl ReadThroughStorage {
    pub fn new(inner: Arc<dyn Storage>) -> Self {
        Self {
            inner,
            cache: Mutex::new(ReadThroughCache::default()),
        }
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, ReadThroughCache> {
        self.cache.lock().expect("storage read-through poisoned")
    }

    fn clear(&self) {
        *self.cache() = ReadThroughCache::default();
    }
}

#[async_trait::async_trait]
impl Storage for ReadThroughStorage {
    fn get_credential_store(&self, connection_id: &Id) -> Result<Option<Box<dyn CredentialStore>>> {
        self.inner.get_credential_store(connection_id)
    }

    fn get_account_config(&self, account_id: &Id) -> Result<Option<AccountConfig>> {
        if let Some(config) = self.cache().account_configs.get(account_id) {
            return Ok(config.clone());
        }

        let config = self.inner.get_account_config(account_id)?;
        self.cache()
            .account_configs
            .insert(account_id.clone(), config.clone());
        Ok(config)
    }

    async fn list_connections(&self) -> Result<Vec<Connection>> {
        if let Some(connections) = self.cache().connections.as_ref() {
            return Ok(connections.clone());
        }

        let connections = self.inner.list_connections().await?;
        self.cache().connections = Some(connections.clone());
        Ok(connections)
    }

    async fn get_connection(&self, id: &Id) -> Result<Option<Connection>> {
        self.inner.get_connection(id).await
    }

    async fn save_connection(&self, conn: &Connection) -> Result<()> {
        self.inner.save_connection(conn).await?;
        self.clear();
        Ok(())
    }

    async fn delete_connection(&self, id: &Id) -> Result<bool> {
        let deleted = self.inner.delete_connection(id).await?;
        self.clear();
        Ok(deleted)
    }

    async fn save_connection_config(&self, id: &Id, config: &ConnectionConfig) -> Result<()> {
        self.inner.save_connection_config(id, config).await?;
        self.clear();
        Ok(())
    }

    async fn list_accounts(&self) -> Result<Vec<Account>> {
        if let Some(accounts) = self.cache().accounts.as_ref() {
            return Ok(accounts.clone());
        }

        let accounts = self.inner.list_accounts().await?;
        self.cache().accounts = Some(accounts.clone());
        Ok(accounts)
    }

    async fn get_account(&self, id: &Id) -> Result<Option<Account>> {
        self.inner.get_account(id).await
    }

    async fn save_account(&self, account: &Account) -> Result<()> {
        self.inner.save_account(account).await?;
        self.clear();
        Ok(())
    }

    async fn delete_account(&self, id: &Id) -> Result<bool> {
        let deleted = self.inner.delete_account(id).await?;
        self.clear();
        Ok(deleted)
    }

    async fn save_account_config(&self, id: &Id, config: &AccountConfig) -> Result<()> {
        self.inner.save_account_config(id, config).await?;
        self.clear();
        Ok(())
    }

    async fn get_balance_snapshots(&self, account_id: &Id) -> Result<Vec<BalanceSnapshot>> {
        if let Some(snapshots) = self.cache().balance_snapshots.get(account_id) {
            return Ok(snapshots.clone());
        }

        let snapshots = self.inner.get_balance_snapshots(account_id).await?;
        self.cache()
            .balance_snapshots
            .insert(account_id.clone(), snapshots.clone());
        Ok(snapshots)
    }

    async fn append_balance_snapshot(
        &self,
        account_id: &Id,
        snapshot: &BalanceSnapshot,
    ) -> Result<()> {
        self.inner
            .append_balance_snapshot(account_id, snapshot)
            .await?;
        self.clear();
        Ok(())
    }

    async fn get_latest_balance_snapshot(
        &self,
        account_id: &Id,
    ) -> Result<Option<BalanceSnapshot>> {
        self.inner.get_latest_balance_snapshot(account_id).await
    }

    async fn get_latest_balances(&self) -> Result<Vec<(Id, BalanceSnapshot)>> {
        self.inner.get_latest_balances().await
    }

    async fn get_latest_balances_for_connection(
        &self,
        connection_id: &Id,
    ) -> Result<Vec<(Id, BalanceSnapshot)>> {
        self.inner
            .get_latest_balances_for_connection(connection_id)
            .await
    }

    async fn get_transactions(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        if let Some(transactions) = self.cache().transactions.get(account_id) {
            return Ok(transactions.clone());
        }

        let transactions = self.inner.get_transactions(account_id).await?;
        self.cache()
            .transactions
            .insert(account_id.clone(), transactions.clone());
        Ok(transactions)
    }

    async fn get_transactions_raw(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        self.inner.get_transactions_raw(account_id).await
    }

    async fn append_transactions(&self, account_id: &Id, txns: &[Transaction]) -> Result<()> {
        self.inner.append_transactions(account_id, txns).await?;
        self.clear();
        Ok(())
    }

    async fn get_transaction_annotation_patches(
        &self,
        account_id: &Id,
    ) -> Result<Vec<TransactionAnnotationPatch>> {
        self.inner
            .get_transaction_annotation_patches(account_id)
            .await
    }

    async fn append_transaction_annotation_patches(
        &self,
        account_id: &Id,
        patches: &[TransactionAnnotationPatch],
    ) -> Result<()> {
        self.inner
            .append_transaction_annotation_patches(account_id, patches)
            .await?;
        self.clear();
        Ok(())
    }

    async fn get_proposed_transaction_edits(&self) -> Result<Vec<ProposedTransactionEdit>> {
        self.inner.get_proposed_transaction_edits().await
    }

    async fn append_proposed_transaction_edits(
        &self,
        edits: &[ProposedTransactionEdit],
    ) -> Result<()> {
        self.inner.append_proposed_transaction_edits(edits).await?;
        self.clear();
        Ok(())
    }

    async fn get_recurring_transaction_reviews(&self) -> Result<Vec<RecurringTransactionReview>> {
        self.inner.get_recurring_transaction_reviews().await
    }

    async fn append_recurring_transaction_reviews(
        &self,
        reviews: &[RecurringTransactionReview],
    ) -> Result<()> {
        self.inner
            .append_recurring_transaction_reviews(reviews)
            .await?;
        self.clear();
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/storage/read_through_tests.rs"]
mod read_through_tests;
