//! Plaid API synchronizer.
//!
//! Supports account balances plus incremental transaction sync via
//! Plaid's `/transactions/sync` cursor API.

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::credentials::CredentialStore;
use crate::models::{
    Account, Asset, AssetBalance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
    Transaction, TransactionStatus,
};
use crate::storage::Storage;
use crate::sync::{SyncResult, SyncedAssetBalance, Synchronizer};

const PLAID_SANDBOX_BASE: &str = "https://sandbox.plaid.com";
const PLAID_DEVELOPMENT_BASE: &str = "https://development.plaid.com";
const PLAID_PRODUCTION_BASE: &str = "https://production.plaid.com";

const TX_SYNC_PAGE_SIZE: u32 = 500;
const TX_SYNC_MAX_PAGES: usize = 200;

const KEY_CLIENT_ID: [&str; 2] = ["client_id", "client-id"];
const KEY_SECRET: [&str; 1] = ["secret"];
const KEY_ACCESS_TOKEN: [&str; 2] = ["access_token", "access-token"];
const KEY_ENVIRONMENT: [&str; 2] = ["environment", "env"];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlaidEnvironment {
    Sandbox,
    Development,
    Production,
}

impl PlaidEnvironment {
    fn base_url(self) -> &'static str {
        match self {
            Self::Sandbox => PLAID_SANDBOX_BASE,
            Self::Development => PLAID_DEVELOPMENT_BASE,
            Self::Production => PLAID_PRODUCTION_BASE,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Sandbox => "sandbox",
            Self::Development => "development",
            Self::Production => "production",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "sandbox" => Ok(Self::Sandbox),
            "development" => Ok(Self::Development),
            "production" => Ok(Self::Production),
            other => anyhow::bail!(
                "Invalid Plaid environment: {other}. Expected sandbox, development, or production."
            ),
        }
    }
}

/// Plaid API synchronizer.
pub struct PlaidSynchronizer {
    client_id: SecretString,
    secret: SecretString,
    access_token: Option<SecretString>,
    environment: PlaidEnvironment,
    base_url: String,
    client: Client,
}

impl PlaidSynchronizer {
    /// Create a synchronizer with explicit credentials.
    pub fn new(
        client_id: SecretString,
        secret: SecretString,
        environment: PlaidEnvironment,
    ) -> Self {
        Self {
            client_id,
            secret,
            access_token: None,
            environment,
            base_url: environment.base_url().to_string(),
            client: Client::new(),
        }
    }

    /// Override API base URL (useful for tests).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Provide an access token directly (useful for tests).
    pub fn with_access_token(mut self, access_token: SecretString) -> Self {
        self.access_token = Some(access_token);
        self
    }

    /// Create from credential store.
    ///
    /// Expected keys:
    /// - `client_id` (or `client-id`)
    /// - `secret`
    /// Optional:
    /// - `access_token` (or `access-token`)
    /// - `environment` (`sandbox`, `development`, `production`)
    pub async fn from_credentials(store: &dyn CredentialStore) -> Result<Self> {
        Self::from_credentials_with_environment(store, None).await
    }

    /// Create from a Keepbook connection.
    pub async fn from_connection<S: Storage + ?Sized>(
        connection: &Connection,
        storage: &S,
    ) -> Result<Self> {
        let credential_store = storage
            .get_credential_store(connection.id())?
            .context("No credentials configured for this connection")?;

        Self::from_credentials_with_environment(
            credential_store.as_ref(),
            Self::connection_environment_override(connection),
        )
        .await
    }

    async fn from_credentials_with_environment(
        store: &dyn CredentialStore,
        environment_override: Option<PlaidEnvironment>,
    ) -> Result<Self> {
        let client_id = get_required_secret(store, &KEY_CLIENT_ID, "client id").await?;
        let secret = get_required_secret(store, &KEY_SECRET, "secret").await?;
        let access_token = get_optional_secret(store, &KEY_ACCESS_TOKEN).await?;

        let environment = if let Some(environment_override) = environment_override {
            environment_override
        } else if let Some(raw_env) = get_optional_secret(store, &KEY_ENVIRONMENT).await? {
            PlaidEnvironment::parse(raw_env.expose_secret())?
        } else {
            PlaidEnvironment::Production
        };

        Ok(Self::new(client_id, secret, environment).with_optional_access_token(access_token))
    }

    fn with_optional_access_token(mut self, access_token: Option<SecretString>) -> Self {
        self.access_token = access_token;
        self
    }

    fn connection_environment_override(connection: &Connection) -> Option<PlaidEnvironment> {
        let raw = connection
            .state
            .synchronizer_data
            .get("environment")
            .and_then(|v| v.as_str())?;

        match PlaidEnvironment::parse(raw) {
            Ok(env) => Some(env),
            Err(err) => {
                tracing::warn!(error = %err, "Ignoring invalid Plaid environment in connection state");
                None
            }
        }
    }

    fn resolve_access_token(&self, connection: &Connection) -> Result<String> {
        if let Some(token) = connection
            .state
            .synchronizer_data
            .get("access_token")
            .and_then(|v| v.as_str())
        {
            return Ok(token.to_string());
        }

        if let Some(token) = &self.access_token {
            return Ok(token.expose_secret().to_string());
        }

        anyhow::bail!(
            "No Plaid access token configured. Set synchronizer_data.access_token or credentials key access_token."
        );
    }

    async fn request<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("Plaid HTTP request failed")?;

        let status = response.status();
        let body_text = response
            .text()
            .await
            .context("Failed to read Plaid response body")?;

        if !status.is_success() {
            anyhow::bail!("Plaid API request failed ({status}): {body_text}");
        }

        serde_json::from_str(&body_text).context("Failed to parse Plaid JSON response")
    }

    async fn get_balances(&self, access_token: &str) -> Result<Vec<PlaidAccount>> {
        #[derive(Serialize)]
        struct Request<'a> {
            client_id: &'a str,
            secret: &'a str,
            access_token: &'a str,
        }

        #[derive(Deserialize)]
        struct Response {
            accounts: Vec<PlaidAccount>,
        }

        let response: Response = self
            .request(
                "/accounts/balance/get",
                &Request {
                    client_id: self.client_id.expose_secret(),
                    secret: self.secret.expose_secret(),
                    access_token,
                },
            )
            .await?;

        Ok(response.accounts)
    }

    async fn get_transaction_updates(
        &self,
        access_token: &str,
        cursor: Option<&str>,
    ) -> Result<TransactionUpdates> {
        #[derive(Serialize)]
        struct Request<'a> {
            client_id: &'a str,
            secret: &'a str,
            access_token: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            cursor: Option<&'a str>,
            count: u32,
        }

        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut removed = Vec::new();
        let mut next_cursor = cursor.map(str::to_string);

        for _ in 0..TX_SYNC_MAX_PAGES {
            let response: PlaidTransactionsSyncResponse = self
                .request(
                    "/transactions/sync",
                    &Request {
                        client_id: self.client_id.expose_secret(),
                        secret: self.secret.expose_secret(),
                        access_token,
                        cursor: next_cursor.as_deref(),
                        count: TX_SYNC_PAGE_SIZE,
                    },
                )
                .await?;

            added.extend(response.added);
            modified.extend(response.modified);
            removed.extend(response.removed);
            next_cursor = Some(response.next_cursor.clone());

            if !response.has_more {
                return Ok(TransactionUpdates {
                    added,
                    modified,
                    removed,
                    next_cursor: response.next_cursor,
                });
            }
        }

        anyhow::bail!(
            "Plaid /transactions/sync returned too many pages (>{TX_SYNC_MAX_PAGES}); aborting."
        );
    }

    async fn load_existing_plaid_transactions<S: Storage + ?Sized>(
        &self,
        connection_id: &Id,
        storage: &S,
    ) -> Result<HashMap<String, ExistingPlaidTransaction>> {
        let existing_accounts = storage.list_accounts().await?;
        let mut existing_by_plaid_id = HashMap::new();

        for account in existing_accounts
            .into_iter()
            .filter(|account| &account.connection_id == connection_id)
        {
            let account_id = account.id.clone();
            let transactions = storage.get_transactions(&account.id).await?;
            for transaction in transactions {
                let Some(plaid_tx_id) = transaction
                    .synchronizer_data
                    .get("plaid_transaction_id")
                    .and_then(|v| v.as_str())
                else {
                    continue;
                };

                existing_by_plaid_id.insert(
                    plaid_tx_id.to_string(),
                    ExistingPlaidTransaction {
                        account_id: account_id.clone(),
                        transaction,
                    },
                );
            }
        }

        Ok(existing_by_plaid_id)
    }

    async fn sync_internal<S: Storage + ?Sized>(
        &self,
        connection: &mut Connection,
        storage: &S,
    ) -> Result<SyncResult> {
        let access_token = self.resolve_access_token(connection)?;
        let plaid_accounts = self.get_balances(&access_token).await?;

        let existing_accounts = storage.list_accounts().await?;
        let existing_by_id: HashMap<Id, Account> = existing_accounts
            .into_iter()
            .filter(|a| a.connection_id == *connection.id())
            .map(|a| (a.id.clone(), a))
            .collect();

        let mut accounts = Vec::new();
        let mut balances: Vec<(Id, Vec<SyncedAssetBalance>)> = Vec::new();
        let mut account_ids_by_plaid: HashMap<String, Id> = HashMap::new();
        let mut account_currency_by_plaid: HashMap<String, String> = HashMap::new();

        for plaid_account in plaid_accounts {
            let account_id = Id::from_external(&plaid_account.account_id);
            account_ids_by_plaid.insert(plaid_account.account_id.clone(), account_id.clone());

            let currency = plaid_account
                .balances
                .iso_currency_code
                .clone()
                .unwrap_or_else(|| "USD".to_string());
            account_currency_by_plaid.insert(plaid_account.account_id.clone(), currency.clone());

            let created_at = existing_by_id
                .get(&account_id)
                .map(|a| a.created_at)
                .unwrap_or_else(Utc::now);

            let mut tags = vec!["plaid".to_string(), plaid_account.account_type.clone()];
            if let Some(subtype) = &plaid_account.subtype {
                if !subtype.is_empty() {
                    tags.push(subtype.clone());
                }
            }

            let account = Account {
                id: account_id.clone(),
                name: plaid_account.name.clone(),
                connection_id: connection.id().clone(),
                tags,
                created_at,
                active: true,
                synchronizer_data: serde_json::json!({
                    "plaid_account_id": plaid_account.account_id,
                    "mask": plaid_account.mask,
                    "official_name": plaid_account.official_name,
                }),
            };

            let asset_balance = AssetBalance::new(
                Asset::currency(&currency),
                plaid_account
                    .balances
                    .current
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "0".to_string()),
            );

            accounts.push(account);
            balances.push((account_id, vec![SyncedAssetBalance::new(asset_balance)]));
        }

        let existing_plaid_transactions = self
            .load_existing_plaid_transactions(connection.id(), storage)
            .await?;

        let cursor = connection
            .state
            .synchronizer_data
            .get("transactions_cursor")
            .and_then(|v| v.as_str());

        let transaction_updates = match self.get_transaction_updates(&access_token, cursor).await {
            Ok(updates) => Some(updates),
            Err(err) => {
                tracing::warn!(error = %err, "Failed to sync Plaid transactions; continuing with balances");
                None
            }
        };

        let mut tx_by_account: HashMap<Id, Vec<Transaction>> = HashMap::new();
        if let Some(updates) = &transaction_updates {
            for plaid_tx in updates.added.iter().chain(updates.modified.iter()) {
                let Some(account_id) = resolve_account_id(
                    &plaid_tx.account_id,
                    &account_ids_by_plaid,
                    &existing_by_id,
                ) else {
                    tracing::warn!(
                        plaid_account_id = %plaid_tx.account_id,
                        plaid_transaction_id = %plaid_tx.transaction_id,
                        "Skipping Plaid transaction for unknown account",
                    );
                    continue;
                };

                let account_currency = account_currency_by_plaid.get(&plaid_tx.account_id);
                let transaction = plaid_transaction_to_keepbook(plaid_tx, account_currency);
                tx_by_account
                    .entry(account_id)
                    .or_default()
                    .push(transaction);
            }

            for removed in &updates.removed {
                let Some(existing) = existing_plaid_transactions.get(&removed.transaction_id)
                else {
                    tracing::warn!(
                        plaid_transaction_id = %removed.transaction_id,
                        "Skipping Plaid removed transaction because it does not exist locally",
                    );
                    continue;
                };

                let mut data = existing
                    .transaction
                    .synchronizer_data
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                data.insert(
                    "plaid_transaction_id".to_string(),
                    serde_json::Value::String(removed.transaction_id.clone()),
                );
                data.insert("removed".to_string(), serde_json::Value::Bool(true));

                let canceled = existing
                    .transaction
                    .clone()
                    .with_status(TransactionStatus::Canceled)
                    .with_synchronizer_data(serde_json::Value::Object(data));

                tx_by_account
                    .entry(existing.account_id.clone())
                    .or_default()
                    .push(canceled);
            }
        }

        let mut transactions: Vec<(Id, Vec<Transaction>)> = tx_by_account.into_iter().collect();
        transactions.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

        connection.state.account_ids = accounts.iter().map(|a| a.id.clone()).collect();
        connection.state.last_sync = Some(LastSync {
            at: Utc::now(),
            status: SyncStatus::Success,
            error: None,
        });
        connection.state.status = ConnectionStatus::Active;

        let mut sync_data = connection
            .state
            .synchronizer_data
            .as_object()
            .cloned()
            .unwrap_or_default();
        sync_data.insert(
            "environment".to_string(),
            serde_json::Value::String(self.environment.as_str().to_string()),
        );
        if let Some(updates) = transaction_updates {
            sync_data.insert(
                "transactions_cursor".to_string(),
                serde_json::Value::String(updates.next_cursor),
            );
        }
        connection.state.synchronizer_data = serde_json::Value::Object(sync_data);

        Ok(SyncResult {
            connection: connection.clone(),
            accounts,
            balances,
            transactions,
        })
    }

    /// Sync with storage access for account lookups.
    pub async fn sync_with_storage<S: Storage + ?Sized>(
        &self,
        connection: &mut Connection,
        storage: &S,
    ) -> Result<SyncResult> {
        self.sync_internal(connection, storage).await
    }
}

fn resolve_account_id(
    plaid_account_id: &str,
    account_ids_by_plaid: &HashMap<String, Id>,
    existing_by_id: &HashMap<Id, Account>,
) -> Option<Id> {
    if let Some(account_id) = account_ids_by_plaid.get(plaid_account_id) {
        return Some(account_id.clone());
    }

    let inferred = Id::from_external(plaid_account_id);
    if existing_by_id.contains_key(&inferred) {
        Some(inferred)
    } else {
        None
    }
}

fn plaid_transaction_to_keepbook(
    tx: &PlaidTransaction,
    account_currency: Option<&String>,
) -> Transaction {
    let timestamp = parse_transaction_timestamp(tx);
    let currency = tx
        .iso_currency_code
        .as_deref()
        .or(tx.unofficial_currency_code.as_deref())
        .or(account_currency.map(|s| s.as_str()))
        .unwrap_or("USD");

    let mut data = serde_json::Map::new();
    data.insert(
        "plaid_transaction_id".to_string(),
        serde_json::Value::String(tx.transaction_id.clone()),
    );
    data.insert("pending".to_string(), serde_json::Value::Bool(tx.pending));
    if let Some(category) = &tx.category {
        data.insert("category".to_string(), serde_json::json!(category));
    }
    if let Some(merchant_name) = &tx.merchant_name {
        data.insert(
            "merchant_name".to_string(),
            serde_json::Value::String(merchant_name.clone()),
        );
    }

    Transaction::new(
        (-tx.amount).to_string(),
        Asset::currency(currency),
        tx.name.clone(),
    )
    .with_timestamp(timestamp)
    .with_status(if tx.pending {
        TransactionStatus::Pending
    } else {
        TransactionStatus::Posted
    })
    .with_id(Id::from_external(&format!(
        "plaid:tx:{}",
        tx.transaction_id
    )))
    .with_synchronizer_data(serde_json::Value::Object(data))
}

fn parse_transaction_timestamp(tx: &PlaidTransaction) -> DateTime<Utc> {
    for datetime in [tx.datetime.as_deref(), tx.authorized_datetime.as_deref()] {
        if let Some(value) = datetime {
            if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
                return parsed.with_timezone(&Utc);
            }
        }
    }

    for date in [tx.date.as_deref(), tx.authorized_date.as_deref()] {
        if let Some(value) = date {
            if let Ok(parsed) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
                if let Some(noon) = parsed.and_hms_opt(12, 0, 0) {
                    return DateTime::from_naive_utc_and_offset(noon, Utc);
                }
            }
        }
    }

    DateTime::from_timestamp(0, 0).unwrap_or_else(Utc::now)
}

#[async_trait::async_trait]
impl Synchronizer for PlaidSynchronizer {
    fn name(&self) -> &str {
        "plaid"
    }

    async fn sync(&self, connection: &mut Connection, storage: &dyn Storage) -> Result<SyncResult> {
        self.sync_internal(connection, storage).await
    }
}

#[derive(Debug, Clone)]
struct TransactionUpdates {
    added: Vec<PlaidTransaction>,
    modified: Vec<PlaidTransaction>,
    removed: Vec<PlaidRemovedTransaction>,
    next_cursor: String,
}

#[derive(Debug, Clone)]
struct ExistingPlaidTransaction {
    account_id: Id,
    transaction: Transaction,
}

#[derive(Debug, Deserialize)]
struct PlaidAccount {
    account_id: String,
    name: String,
    #[serde(rename = "type")]
    account_type: String,
    subtype: Option<String>,
    mask: Option<String>,
    official_name: Option<String>,
    balances: PlaidBalances,
}

#[derive(Debug, Deserialize)]
struct PlaidBalances {
    current: Option<f64>,
    iso_currency_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PlaidTransaction {
    transaction_id: String,
    account_id: String,
    amount: f64,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    datetime: Option<String>,
    #[serde(default)]
    authorized_date: Option<String>,
    #[serde(default)]
    authorized_datetime: Option<String>,
    name: String,
    #[serde(default)]
    pending: bool,
    category: Option<Vec<String>>,
    merchant_name: Option<String>,
    iso_currency_code: Option<String>,
    unofficial_currency_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct PlaidRemovedTransaction {
    transaction_id: String,
}

#[derive(Debug, Deserialize)]
struct PlaidTransactionsSyncResponse {
    #[serde(default)]
    added: Vec<PlaidTransaction>,
    #[serde(default)]
    modified: Vec<PlaidTransaction>,
    #[serde(default)]
    removed: Vec<PlaidRemovedTransaction>,
    #[serde(default)]
    has_more: bool,
    next_cursor: String,
}

async fn get_optional_secret(
    store: &dyn CredentialStore,
    keys: &[&str],
) -> Result<Option<SecretString>> {
    for key in keys {
        if let Some(value) = store.get(key).await? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

async fn get_required_secret(
    store: &dyn CredentialStore,
    keys: &[&str],
    name: &str,
) -> Result<SecretString> {
    get_optional_secret(store, keys).await?.with_context(|| {
        format!(
            "Missing Plaid {name} in credentials (expected one of: {})",
            keys.join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ConnectionConfig, TransactionStatus};
    use crate::storage::MemoryStorage;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn sync_updates_cursor_and_handles_removed_transactions() -> Result<()> {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/accounts/balance/get"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accounts": [{
                    "account_id": "acc_1",
                    "name": "Checking",
                    "type": "depository",
                    "subtype": "checking",
                    "mask": "0000",
                    "official_name": "Primary Checking",
                    "balances": {
                        "current": 1000.25,
                        "iso_currency_code": "USD"
                    }
                }]
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/transactions/sync"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "added": [{
                    "transaction_id": "tx_added",
                    "account_id": "acc_1",
                    "amount": 12.34,
                    "date": "2026-02-10",
                    "name": "Coffee",
                    "pending": false,
                    "iso_currency_code": "USD"
                }],
                "modified": [],
                "removed": [{
                    "transaction_id": "tx_removed"
                }],
                "has_more": false,
                "next_cursor": "cursor-1"
            })))
            .mount(&server)
            .await;

        let storage = MemoryStorage::new();
        let mut connection = Connection::new(ConnectionConfig {
            name: "Plaid".to_string(),
            synchronizer: "plaid".to_string(),
            credentials: None,
            balance_staleness: None,
        });
        connection.state.synchronizer_data = serde_json::json!({
            "access_token": "access-token-1",
            "transactions_cursor": "cursor-0",
        });

        let existing_account_id = Id::from_external("acc_1");
        let existing_account = Account {
            id: existing_account_id.clone(),
            name: "Existing Checking".to_string(),
            connection_id: connection.id().clone(),
            tags: vec!["plaid".to_string()],
            created_at: Utc::now() - chrono::Duration::days(30),
            active: true,
            synchronizer_data: serde_json::json!({
                "plaid_account_id": "acc_1"
            }),
        };
        storage.save_account(&existing_account).await?;

        let removed_tx = Transaction::new("-5", Asset::currency("USD"), "Old Tx")
            .with_id(Id::from_external("plaid:tx:tx_removed"))
            .with_status(TransactionStatus::Posted)
            .with_synchronizer_data(serde_json::json!({
                "plaid_transaction_id": "tx_removed",
            }));
        storage
            .append_transactions(&existing_account_id, &[removed_tx])
            .await?;

        let synchronizer = PlaidSynchronizer::new(
            SecretString::new("client-id".to_string().into()),
            SecretString::new("secret".to_string().into()),
            PlaidEnvironment::Sandbox,
        )
        .with_base_url(server.uri());

        let result = synchronizer.sync(&mut connection, &storage).await?;

        let synced_account = result
            .accounts
            .iter()
            .find(|a| a.id == existing_account_id)
            .context("Expected account in sync result")?;
        assert_eq!(synced_account.created_at, existing_account.created_at);

        let account_txs = result
            .transactions
            .iter()
            .find(|(id, _)| *id == existing_account_id)
            .map(|(_, txs)| txs)
            .context("Expected transactions for existing account")?;

        let added = account_txs
            .iter()
            .find(|tx| {
                tx.synchronizer_data
                    .get("plaid_transaction_id")
                    .and_then(|v| v.as_str())
                    == Some("tx_added")
            })
            .context("Expected added transaction")?;
        assert_eq!(added.id, Id::from_external("plaid:tx:tx_added"));
        assert_eq!(added.amount, "-12.34");
        assert_eq!(added.status, TransactionStatus::Posted);

        let canceled = account_txs
            .iter()
            .find(|tx| {
                tx.synchronizer_data
                    .get("plaid_transaction_id")
                    .and_then(|v| v.as_str())
                    == Some("tx_removed")
            })
            .context("Expected canceled transaction")?;
        assert_eq!(canceled.id, Id::from_external("plaid:tx:tx_removed"));
        assert_eq!(canceled.status, TransactionStatus::Canceled);
        assert_eq!(
            canceled.synchronizer_data.get("removed"),
            Some(&serde_json::json!(true))
        );

        assert_eq!(
            result.connection.state.synchronizer_data["transactions_cursor"],
            serde_json::json!("cursor-1")
        );
        assert_eq!(
            result.connection.state.synchronizer_data["environment"],
            serde_json::json!("sandbox")
        );

        Ok(())
    }

    #[tokio::test]
    async fn sync_without_access_token_fails_fast() {
        let storage = MemoryStorage::new();
        let mut connection = Connection::new(ConnectionConfig {
            name: "Plaid".to_string(),
            synchronizer: "plaid".to_string(),
            credentials: None,
            balance_staleness: None,
        });

        let synchronizer = PlaidSynchronizer::new(
            SecretString::new("client-id".to_string().into()),
            SecretString::new("secret".to_string().into()),
            PlaidEnvironment::Sandbox,
        );

        let err = synchronizer
            .sync(&mut connection, &storage)
            .await
            .expect_err("sync should fail without access token");
        assert!(err.to_string().contains("No Plaid access token configured"));
    }
}
