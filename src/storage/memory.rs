// src/storage/memory.rs
//! In-memory storage implementation for testing.

use std::collections::HashMap;

use anyhow::Result;
use tokio::sync::Mutex;

use crate::credentials::CredentialStore;
use crate::models::{Account, Asset, Balance, Connection, Id, Transaction};

use super::Storage;

/// In-memory storage for testing purposes.
pub struct MemoryStorage {
    connections: Mutex<HashMap<Id, Connection>>,
    accounts: Mutex<HashMap<Id, Account>>,
    balances: Mutex<HashMap<Id, Vec<Balance>>>,
    transactions: Mutex<HashMap<Id, Vec<Transaction>>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            accounts: Mutex::new(HashMap::new()),
            balances: Mutex::new(HashMap::new()),
            transactions: Mutex::new(HashMap::new()),
        }
    }

    /// Convenience method for tests to save a single balance.
    pub async fn save_balance(&self, account_id: &Id, balance: &Balance) -> Result<()> {
        self.append_balances(account_id, &[balance.clone()]).await
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Storage for MemoryStorage {
    fn get_credential_store(&self, _connection_id: &Id) -> Result<Option<Box<dyn CredentialStore>>> {
        Ok(None)
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

    async fn delete_account(&self, id: &Id) -> Result<bool> {
        let mut accounts = self.accounts.lock().await;
        Ok(accounts.remove(id).is_some())
    }

    async fn get_balances(&self, account_id: &Id) -> Result<Vec<Balance>> {
        let balances = self.balances.lock().await;
        Ok(balances.get(account_id).cloned().unwrap_or_default())
    }

    async fn append_balances(&self, account_id: &Id, new_balances: &[Balance]) -> Result<()> {
        let mut balances = self.balances.lock().await;
        balances
            .entry(account_id.clone())
            .or_default()
            .extend(new_balances.iter().cloned());
        Ok(())
    }

    async fn get_latest_balances(&self) -> Result<Vec<(Id, Balance)>> {
        let accounts = self.accounts.lock().await;
        let balances = self.balances.lock().await;

        let mut results = Vec::new();
        for account_id in accounts.keys() {
            if let Some(account_balances) = balances.get(account_id) {
                // Group by asset, keep most recent
                let mut latest: HashMap<Asset, Balance> = HashMap::new();
                for balance in account_balances {
                    latest
                        .entry(balance.asset.clone())
                        .and_modify(|existing| {
                            if balance.timestamp > existing.timestamp {
                                *existing = balance.clone();
                            }
                        })
                        .or_insert(balance.clone());
                }
                for balance in latest.into_values() {
                    results.push((account_id.clone(), balance));
                }
            }
        }

        Ok(results)
    }

    async fn get_latest_balances_for_connection(&self, connection_id: &Id) -> Result<Vec<(Id, Balance)>> {
        let connections = self.connections.lock().await;
        let accounts = self.accounts.lock().await;
        let balances = self.balances.lock().await;

        let connection = connections.get(connection_id);
        if connection.is_none() {
            return Ok(Vec::new());
        }

        // Find all accounts for this connection
        let account_ids: Vec<Id> = accounts
            .values()
            .filter(|a| &a.connection_id == connection_id)
            .map(|a| a.id.clone())
            .collect();

        let mut results = Vec::new();
        for account_id in account_ids {
            if let Some(account_balances) = balances.get(&account_id) {
                let mut latest: HashMap<Asset, Balance> = HashMap::new();
                for balance in account_balances {
                    latest
                        .entry(balance.asset.clone())
                        .and_modify(|existing| {
                            if balance.timestamp > existing.timestamp {
                                *existing = balance.clone();
                            }
                        })
                        .or_insert(balance.clone());
                }
                for balance in latest.into_values() {
                    results.push((account_id.clone(), balance));
                }
            }
        }

        Ok(results)
    }

    async fn get_latest_balances_for_account(&self, account_id: &Id) -> Result<Vec<Balance>> {
        let balances = self.balances.lock().await;

        if let Some(account_balances) = balances.get(account_id) {
            let mut latest: HashMap<Asset, Balance> = HashMap::new();
            for balance in account_balances {
                latest
                    .entry(balance.asset.clone())
                    .and_modify(|existing| {
                        if balance.timestamp > existing.timestamp {
                            *existing = balance.clone();
                        }
                    })
                    .or_insert(balance.clone());
            }
            Ok(latest.into_values().collect())
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_transactions(&self, account_id: &Id) -> Result<Vec<Transaction>> {
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
}
