mod json_file;
pub mod lookup;
mod memory;

pub use json_file::JsonFileStorage;
pub use lookup::{find_account, find_connection};
pub use memory::MemoryStorage;

use crate::credentials::CredentialStore;
use crate::models::{
    Account, AccountConfig, BalanceSnapshot, Connection, ConnectionConfig, Id, Transaction,
    TransactionAnnotationPatch,
};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

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
    /// Get the raw append-only transaction history (may include duplicates / multiple versions).
    ///
    /// Most callers should prefer `get_transactions` which returns a last-write-wins view.
    async fn get_transactions_raw(&self, account_id: &Id) -> Result<Vec<Transaction>> {
        self.get_transactions(account_id).await
    }
    async fn append_transactions(&self, account_id: &Id, txns: &[Transaction]) -> Result<()>;

    // Transaction annotations (append-only patches)
    async fn get_transaction_annotation_patches(
        &self,
        account_id: &Id,
    ) -> Result<Vec<TransactionAnnotationPatch>>;
    async fn append_transaction_annotation_patches(
        &self,
        account_id: &Id,
        patches: &[TransactionAnnotationPatch],
    ) -> Result<()>;
}

/// Filesystem-y operations that only make sense for the JSON file layout.
#[async_trait::async_trait]
pub trait SymlinkStorage: Send + Sync {
    async fn rebuild_all_symlinks(&self) -> Result<(usize, usize, Vec<String>)>;
}

#[async_trait::async_trait]
impl SymlinkStorage for JsonFileStorage {
    async fn rebuild_all_symlinks(&self) -> Result<(usize, usize, Vec<String>)> {
        JsonFileStorage::rebuild_all_symlinks(self).await
    }
}

fn non_empty_sync_string(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn transaction_dedupe_keys(txn: &Transaction) -> Vec<String> {
    let mut keys = vec![format!("id:{}", txn.id)];

    // Chase sometimes surfaces the same transaction under different stable id sources
    // (e.g. derived_unique_transaction_identifier vs sor_transaction_identifier).
    // Treat these as aliases so we don't double-count existing append-only history.
    if let serde_json::Value::Object(obj) = &txn.synchronizer_data {
        if obj.contains_key("chase_account_id") {
            for field in [
                "stable_id",
                "sor_transaction_identifier",
                "derived_unique_transaction_identifier",
                "transaction_reference_number",
            ] {
                if let Some(value) = non_empty_sync_string(obj, field) {
                    keys.push(format!("chase:{field}:{value}"));
                    keys.push(format!("chase:alias:{value}"));
                }
            }
        }
    }

    keys.sort();
    keys.dedup();
    keys
}

pub(crate) fn dedupe_transactions_last_write_wins(txns: Vec<Transaction>) -> Vec<Transaction> {
    let mut key_to_index: HashMap<String, usize> = HashMap::new();
    let mut index_to_keys: HashMap<usize, HashSet<String>> = HashMap::new();
    let mut deduped: Vec<Option<Transaction>> = Vec::new();

    for txn in txns {
        let keys = transaction_dedupe_keys(&txn);
        let mut matched: HashSet<usize> = HashSet::new();

        for key in &keys {
            if let Some(idx) = key_to_index.get(key).copied() {
                if deduped.get(idx).and_then(|t| t.as_ref()).is_some() {
                    matched.insert(idx);
                }
            }
        }

        let target_idx = if matched.is_empty() {
            let idx = deduped.len();
            deduped.push(Some(txn));
            idx
        } else {
            let idx = *matched
                .iter()
                .min()
                .expect("matched is non-empty when branch is taken");
            deduped[idx] = Some(txn);
            idx
        };

        for idx in matched {
            if idx == target_idx {
                continue;
            }
            deduped[idx] = None;
            if let Some(keys_for_idx) = index_to_keys.remove(&idx) {
                let target_keys = index_to_keys.entry(target_idx).or_default();
                for key in keys_for_idx {
                    key_to_index.insert(key.clone(), target_idx);
                    target_keys.insert(key);
                }
            }
        }

        let target_keys = index_to_keys.entry(target_idx).or_default();
        for key in keys {
            key_to_index.insert(key.clone(), target_idx);
            target_keys.insert(key);
        }
    }

    deduped.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::dedupe_transactions_last_write_wins;
    use crate::models::{Asset, Id, Transaction, TransactionStatus};
    use chrono::TimeZone;

    fn chase_tx(
        id: &str,
        stable_id: &str,
        sor_id: Option<&str>,
        derived_id: Option<&str>,
    ) -> Transaction {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "chase_account_id".to_string(),
            serde_json::Value::Number(123.into()),
        );
        obj.insert(
            "stable_id".to_string(),
            serde_json::Value::String(stable_id.to_string()),
        );
        if let Some(v) = sor_id {
            obj.insert(
                "sor_transaction_identifier".to_string(),
                serde_json::Value::String(v.to_string()),
            );
        }
        if let Some(v) = derived_id {
            obj.insert(
                "derived_unique_transaction_identifier".to_string(),
                serde_json::Value::String(v.to_string()),
            );
        }

        Transaction {
            id: Id::from_string(id),
            timestamp: chrono::Utc.with_ymd_and_hms(2026, 2, 20, 12, 0, 0).unwrap(),
            amount: "-10".to_string(),
            asset: Asset::currency("USD"),
            description: "Test".to_string(),
            status: TransactionStatus::Posted,
            synchronizer_data: serde_json::Value::Object(obj),
        }
    }

    #[test]
    fn dedupe_transactions_collapses_chase_alias_ids() {
        let old = chase_tx("tx-old", "202602151536556260124#20260124", None, None);
        let new_no_alias = chase_tx("tx-new", "466046216565116", None, None);
        let new_with_alias = chase_tx(
            "tx-new",
            "466046216565116",
            Some("466046216565116"),
            Some("202602151536556260124#20260124"),
        );

        let out = dedupe_transactions_last_write_wins(vec![old, new_no_alias, new_with_alias]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id.as_str(), "tx-new");
    }
}
