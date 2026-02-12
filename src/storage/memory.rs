// src/storage/memory.rs
//! In-memory storage implementation for testing.

use std::collections::HashMap;
use std::sync::Mutex as StdMutex;

use anyhow::Result;
use tokio::sync::Mutex;

use crate::credentials::CredentialStore;
use crate::models::{
    Account, AccountConfig, BalanceSnapshot, Connection, ConnectionConfig, ConnectionState, Id,
    Transaction, TransactionAnnotationPatch,
};

use super::Storage;

/// In-memory storage for testing purposes.
pub struct MemoryStorage {
    connections: Mutex<HashMap<Id, Connection>>,
    accounts: Mutex<HashMap<Id, Account>>,
    account_configs: StdMutex<HashMap<Id, AccountConfig>>,
    balances: Mutex<HashMap<Id, Vec<BalanceSnapshot>>>,
    transactions: Mutex<HashMap<Id, Vec<Transaction>>>,
    transaction_annotation_patches: Mutex<HashMap<Id, Vec<TransactionAnnotationPatch>>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            accounts: Mutex::new(HashMap::new()),
            account_configs: StdMutex::new(HashMap::new()),
            balances: Mutex::new(HashMap::new()),
            transactions: Mutex::new(HashMap::new()),
            transaction_annotation_patches: Mutex::new(HashMap::new()),
        }
    }

    pub async fn set_account_config(&self, account_id: &Id, config: AccountConfig) {
        let mut configs = self
            .account_configs
            .lock()
            .expect("account config lock poisoned");
        configs.insert(account_id.clone(), config);
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Storage for MemoryStorage {
    fn get_credential_store(
        &self,
        _connection_id: &Id,
    ) -> Result<Option<Box<dyn CredentialStore>>> {
        Ok(None)
    }

    fn get_account_config(&self, account_id: &Id) -> Result<Option<AccountConfig>> {
        let configs = self
            .account_configs
            .lock()
            .expect("account config lock poisoned");
        Ok(configs.get(account_id).cloned())
    }

    async fn list_connections(&self) -> Result<Vec<Connection>> {
        let conns = self.connections.lock().await;
        Ok(conns.values().cloned().collect())
    }

    async fn get_connection(&self, id: &Id) -> Result<Option<Connection>> {
        let conns = self.connections.lock().await;
        Ok(conns.get(id).cloned())
    }

    async fn save_connection(&self, conn: &Connection) -> Result<()> {
        let mut conns = self.connections.lock().await;
        conns.insert(conn.id().clone(), conn.clone());
        Ok(())
    }

    async fn delete_connection(&self, id: &Id) -> Result<bool> {
        let mut conns = self.connections.lock().await;
        Ok(conns.remove(id).is_some())
    }

    async fn save_connection_config(&self, id: &Id, config: &ConnectionConfig) -> Result<()> {
        let mut conns = self.connections.lock().await;
        match conns.get_mut(id) {
            Some(existing) => {
                existing.config = config.clone();
            }
            None => {
                conns.insert(
                    id.clone(),
                    Connection {
                        config: config.clone(),
                        state: ConnectionState::new_with(id.clone(), chrono::Utc::now()),
                    },
                );
            }
        }
        Ok(())
    }

    async fn list_accounts(&self) -> Result<Vec<Account>> {
        let accounts = self.accounts.lock().await;
        Ok(accounts.values().cloned().collect())
    }

    async fn get_account(&self, id: &Id) -> Result<Option<Account>> {
        let accounts = self.accounts.lock().await;
        Ok(accounts.get(id).cloned())
    }

    async fn save_account(&self, account: &Account) -> Result<()> {
        let mut accounts = self.accounts.lock().await;
        accounts.insert(account.id.clone(), account.clone());
        Ok(())
    }

    async fn save_account_config(&self, id: &Id, config: &AccountConfig) -> Result<()> {
        let mut configs = self
            .account_configs
            .lock()
            .expect("account config lock poisoned");
        configs.insert(id.clone(), config.clone());
        Ok(())
    }

    async fn delete_account(&self, id: &Id) -> Result<bool> {
        let mut accounts = self.accounts.lock().await;
        Ok(accounts.remove(id).is_some())
    }

    async fn get_balance_snapshots(&self, account_id: &Id) -> Result<Vec<BalanceSnapshot>> {
        let balances = self.balances.lock().await;
        Ok(balances.get(account_id).cloned().unwrap_or_default())
    }

    async fn append_balance_snapshot(
        &self,
        account_id: &Id,
        snapshot: &BalanceSnapshot,
    ) -> Result<()> {
        let mut balances = self.balances.lock().await;
        balances
            .entry(account_id.clone())
            .or_default()
            .push(snapshot.clone());
        Ok(())
    }

    async fn get_latest_balances(&self) -> Result<Vec<(Id, BalanceSnapshot)>> {
        let accounts = self.accounts.lock().await;
        let balances = self.balances.lock().await;

        let mut results = Vec::new();
        for account_id in accounts.keys() {
            if let Some(snapshots) = balances.get(account_id) {
                if let Some(latest) = snapshots.iter().max_by_key(|s| s.timestamp) {
                    results.push((account_id.clone(), latest.clone()));
                }
            }
        }

        Ok(results)
    }

    async fn get_latest_balances_for_connection(
        &self,
        connection_id: &Id,
    ) -> Result<Vec<(Id, BalanceSnapshot)>> {
        let connections = self.connections.lock().await;
        let accounts = self.accounts.lock().await;
        let balances = self.balances.lock().await;

        if connections.get(connection_id).is_none() {
            anyhow::bail!("Connection not found");
        }

        let account_ids: Vec<Id> = accounts
            .values()
            .filter(|a| &a.connection_id == connection_id)
            .map(|a| a.id.clone())
            .collect();

        let mut results = Vec::new();
        for account_id in account_ids {
            if let Some(snapshots) = balances.get(&account_id) {
                if let Some(latest) = snapshots.iter().max_by_key(|s| s.timestamp) {
                    results.push((account_id.clone(), latest.clone()));
                }
            }
        }

        Ok(results)
    }

    async fn get_latest_balance_snapshot(
        &self,
        account_id: &Id,
    ) -> Result<Option<BalanceSnapshot>> {
        let balances = self.balances.lock().await;
        Ok(balances
            .get(account_id)
            .and_then(|snapshots| snapshots.iter().max_by_key(|s| s.timestamp).cloned()))
    }

    async fn get_transactions(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        let txns = self.get_transactions_raw(account_id).await?;

        // Mirror JsonFileStorage behavior: last write wins for duplicate ids.
        let mut by_id: std::collections::HashMap<Id, usize> = std::collections::HashMap::new();
        let mut deduped: Vec<Transaction> = Vec::new();
        for txn in txns {
            if let Some(idx) = by_id.get(&txn.id).copied() {
                deduped[idx] = txn;
            } else {
                by_id.insert(txn.id.clone(), deduped.len());
                deduped.push(txn);
            }
        }

        Ok(deduped)
    }

    async fn get_transactions_raw(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        let txns = self.transactions.lock().await;
        Ok(txns.get(account_id).cloned().unwrap_or_default())
    }

    async fn append_transactions(&self, account_id: &Id, new_txns: &[Transaction]) -> Result<()> {
        let mut txns = self.transactions.lock().await;
        txns.entry(account_id.clone())
            .or_default()
            .extend(new_txns.iter().cloned());
        Ok(())
    }

    async fn get_transaction_annotation_patches(
        &self,
        account_id: &Id,
    ) -> Result<Vec<TransactionAnnotationPatch>> {
        let patches = self.transaction_annotation_patches.lock().await;
        Ok(patches.get(account_id).cloned().unwrap_or_default())
    }

    async fn append_transaction_annotation_patches(
        &self,
        account_id: &Id,
        new_patches: &[TransactionAnnotationPatch],
    ) -> Result<()> {
        if new_patches.is_empty() {
            return Ok(());
        }
        let mut patches = self.transaction_annotation_patches.lock().await;
        patches
            .entry(account_id.clone())
            .or_default()
            .extend(new_patches.iter().cloned());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_storage_errors_on_missing_connection_balances() -> Result<()> {
        let storage = MemoryStorage::new();
        let missing = Id::new();

        let err = storage
            .get_latest_balances_for_connection(&missing)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Connection not found"));

        Ok(())
    }
}
