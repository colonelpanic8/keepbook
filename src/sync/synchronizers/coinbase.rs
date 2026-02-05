//! Coinbase CDP API synchronizer.
//!
//! This synchronizer uses Coinbase's CDP API with JWT authentication.
//! Credentials are loaded via the CredentialStore abstraction.

use std::collections::HashSet;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use p256::SecretKey;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::credentials::CredentialStore;
use crate::models::{
    Account, Asset, AssetBalance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
    Transaction,
};
use crate::storage::Storage;
use crate::sync::{SyncResult, SyncedAssetBalance, Synchronizer};

const CDP_API_BASE: &str = "https://api.coinbase.com";

/// Coinbase CDP API synchronizer.
pub struct CoinbaseSynchronizer {
    key_name: String,
    private_key_pem: SecretString,
    client: Client,
    api_base: String,
}

#[derive(Debug, Serialize)]
struct JwtClaims {
    sub: String,
    iss: String,
    nbf: i64,
    exp: i64,
    uri: String,
}

impl CoinbaseSynchronizer {
    /// Create a new Coinbase synchronizer with explicit credentials.
    pub fn new(key_name: String, private_key_pem: SecretString) -> Self {
        Self {
            key_name,
            private_key_pem,
            client: Client::new(),
            api_base: CDP_API_BASE.to_string(),
        }
    }

    /// Override the API base URL (useful for tests).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.api_base = base_url.into();
        self
    }

    /// Create a synchronizer by loading credentials from storage.
    pub async fn from_credentials(store: &dyn CredentialStore) -> Result<Self> {
        let key_name = store
            .get("key-name")
            .await?
            .context("Missing key-name in credentials")?;

        let private_key = store
            .get("private-key")
            .await?
            .context("Missing private-key in credentials")?;

        Ok(Self::new(key_name.expose_secret().to_string(), private_key))
    }

    fn generate_jwt(&self, method: &str, path: &str) -> Result<String> {
        let now = Utc::now().timestamp();
        let base = self
            .api_base
            .trim_end_matches('/')
            .replace("https://", "")
            .replace("http://", "");
        let uri = format!(
            "{} {}{}",
            method,
            base,
            path
        );

        let claims = JwtClaims {
            sub: self.key_name.clone(),
            iss: "cdp".to_string(),
            nbf: now,
            exp: now + 120, // 2 minute expiry
            uri,
        };

        // Create JWT header
        let header = serde_json::json!({
            "alg": "ES256",
            "typ": "JWT",
            "kid": self.key_name,
            "nonce": format!("{:x}", rand::random::<u64>())
        });

        // Encode header and claims
        let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_string(&header)?);
        let claims_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_string(&claims)?);
        let message = format!("{header_b64}.{claims_b64}");

        // Parse the EC private key (SEC1 format)
        let secret_key = SecretKey::from_sec1_pem(self.private_key_pem.expose_secret())
            .context("Failed to parse EC private key")?;
        let signing_key = SigningKey::from(&secret_key);

        // Sign the message
        let signature: Signature = signing_key.sign(message.as_bytes());
        let sig_bytes = signature.to_bytes();
        let sig_b64 = URL_SAFE_NO_PAD.encode(sig_bytes);

        Ok(format!("{message}.{sig_b64}"))
    }

    async fn request<T: for<'de> Deserialize<'de>>(&self, method: &str, path: &str) -> Result<T> {
        let jwt = self.generate_jwt(method, path)?;
        let base = self.api_base.trim_end_matches('/');
        let url = format!("{base}{path}");

        let response = self
            .client
            .request(method.parse().unwrap(), &url)
            .header("Authorization", format!("Bearer {jwt}"))
            .header("Content-Type", "application/json")
            .send()
            .await
            .context("HTTP request failed")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read response body")?;

        if !status.is_success() {
            anyhow::bail!("API request failed ({status}): {body}");
        }

        serde_json::from_str(&body).context("Failed to parse JSON response")
    }

    async fn get_accounts(&self) -> Result<Vec<CoinbaseAccount>> {
        // Use portfolios breakdown to get all positions including ETH/BTC
        // The accounts endpoint doesn't return all wallets for some reason
        #[derive(Debug, Deserialize)]
        struct Portfolio {
            uuid: String,
            #[allow(dead_code)]
            name: String,
        }
        #[derive(Debug, Deserialize)]
        struct PortfoliosResponse {
            portfolios: Vec<Portfolio>,
        }
        #[derive(Debug, Deserialize)]
        struct SpotPosition {
            asset: String,
            account_uuid: String,
            total_balance_crypto: f64,
            #[serde(default)]
            is_cash: bool,
        }
        #[derive(Debug, Deserialize)]
        struct PortfolioBreakdown {
            #[serde(default)]
            spot_positions: Vec<SpotPosition>,
        }
        #[derive(Debug, Deserialize)]
        struct BreakdownResponse {
            breakdown: PortfolioBreakdown,
        }

        let mut accounts = Vec::new();

        // Get portfolios and their breakdowns
        let portfolios: PortfoliosResponse =
            self.request("GET", "/api/v3/brokerage/portfolios").await?;

        for p in portfolios.portfolios {
            let path = format!("/api/v3/brokerage/portfolios/{}", p.uuid);
            let breakdown: BreakdownResponse = self.request("GET", &path).await?;

            for pos in breakdown.breakdown.spot_positions {
                // Skip fiat currencies (USD, etc) - they're handled differently
                if pos.is_cash {
                    continue;
                }

                accounts.push(CoinbaseAccount {
                    uuid: pos.account_uuid,
                    name: format!("{} Wallet", pos.asset),
                    currency: pos.asset,
                    available_balance: CoinbaseBalance {
                        value: pos.total_balance_crypto.to_string(),
                        currency: String::new(), // Not used
                    },
                    account_type: "ACCOUNT_TYPE_CRYPTO".to_string(),
                });
            }
        }

        tracing::info!(
            total_accounts = accounts.len(),
            "coinbase portfolios API returned accounts"
        );

        Ok(accounts)
    }

    async fn get_transactions(&self, account_id: &str) -> Result<Vec<CoinbaseTransaction>> {
        #[derive(Deserialize)]
        struct Response {
            #[serde(default)]
            ledger: Vec<CoinbaseTransaction>,
        }

        let path = format!("/api/v3/brokerage/accounts/{account_id}/ledger");
        let resp: Response = self.request("GET", &path).await?;
        Ok(resp.ledger)
    }

    async fn sync_internal<S: Storage + ?Sized>(
        &self,
        connection: &mut Connection,
        storage: &S,
    ) -> Result<SyncResult> {
        let coinbase_accounts = self.get_accounts().await?;

        // Load existing accounts to check for history
        let existing_accounts = storage.list_accounts().await?;
        let existing_ids: HashSet<String> = existing_accounts
            .iter()
            .filter(|a| a.connection_id == *connection.id())
            .map(|a| a.id.to_string())
            .collect();

        let mut accounts = Vec::new();
        let mut balances: Vec<(Id, Vec<SyncedAssetBalance>)> = Vec::new();
        let mut transactions: Vec<(Id, Vec<Transaction>)> = Vec::new();

        for cb_account in coinbase_accounts {
            // Use Coinbase's UUID directly as our account ID
            let account_id = Id::from_string(&cb_account.uuid);
            let asset = Asset::crypto(&cb_account.currency);
            let balance_amount: f64 = cb_account.available_balance.value.parse().unwrap_or(0.0);

            tracing::debug!(
                name = %cb_account.name,
                currency = %cb_account.currency,
                balance = %balance_amount,
                "processing coinbase account"
            );

            // Check if account already exists
            let existing = existing_ids.contains(&cb_account.uuid);

            // Get transactions for this account
            let cb_transactions = self
                .get_transactions(&cb_account.uuid)
                .await
                .unwrap_or_else(|e| {
                    eprintln!(
                        "Warning: Failed to get transactions for {}: {}",
                        cb_account.name, e
                    );
                    Vec::new()
                });

            // Skip zero-balance accounts unless they already exist or have transactions
            if balance_amount == 0.0 && !existing && cb_transactions.is_empty() {
                continue;
            }

            // Get existing account's created_at or use now
            let created_at = existing_accounts
                .iter()
                .find(|a| a.id.to_string() == cb_account.uuid)
                .map(|a| a.created_at)
                .unwrap_or_else(Utc::now);

            let account = Account {
                id: account_id.clone(),
                name: cb_account.name.clone(),
                connection_id: connection.id().clone(),
                tags: vec!["coinbase".to_string(), cb_account.account_type.clone()],
                created_at,
                active: true,
                synchronizer_data: serde_json::json!({
                    "currency": cb_account.currency,
                }),
            };

            // Record current balance
            let asset_balance =
                AssetBalance::new(asset.clone(), &cb_account.available_balance.value);

            let account_transactions: Vec<Transaction> = cb_transactions
                .into_iter()
                .filter_map(|tx| {
                    let timestamp = DateTime::parse_from_rfc3339(&tx.created_at)
                        .ok()?
                        .with_timezone(&Utc);

                    Some(
                        Transaction::new(
                            &tx.amount.value,
                            Asset::crypto(&tx.amount.currency),
                            tx.description.unwrap_or_else(|| tx.entry_type.clone()),
                        )
                        .with_timestamp(timestamp)
                        .with_synchronizer_data(serde_json::json!({
                            "coinbase_entry_id": tx.entry_id,
                            "entry_type": tx.entry_type,
                        })),
                    )
                })
                .collect();

            accounts.push(account);
            balances.push((
                account_id.clone(),
                vec![SyncedAssetBalance::new(asset_balance)],
            ));
            transactions.push((account_id, account_transactions));
        }

        // Update connection state
        connection.state.account_ids = accounts.iter().map(|a| a.id.clone()).collect();
        connection.state.last_sync = Some(LastSync {
            at: Utc::now(),
            status: SyncStatus::Success,
            error: None,
        });
        connection.state.status = ConnectionStatus::Active;

        Ok(SyncResult {
            connection: connection.clone(),
            accounts,
            balances,
            transactions,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CoinbaseAccount {
    uuid: String,
    name: String,
    currency: String,
    available_balance: CoinbaseBalance,
    #[serde(rename = "type")]
    account_type: String,
}

#[derive(Debug, Deserialize)]
struct CoinbaseBalance {
    value: String,
    #[allow(dead_code)]
    currency: String,
}

#[derive(Debug, Deserialize)]
struct CoinbaseTransaction {
    entry_id: String,
    entry_type: String,
    amount: CoinbaseBalance,
    created_at: String,
    #[serde(default)]
    description: Option<String>,
}

#[async_trait::async_trait]
impl Synchronizer for CoinbaseSynchronizer {
    fn name(&self) -> &str {
        "coinbase"
    }

    async fn sync(&self, connection: &mut Connection, storage: &dyn Storage) -> Result<SyncResult> {
        self.sync_internal(connection, storage).await
    }
}

impl CoinbaseSynchronizer {
    /// Create a new synchronizer from connection credentials.
    pub async fn from_connection<S: Storage + ?Sized>(
        connection: &Connection,
        storage: &S,
    ) -> Result<Self> {
        let credential_store = storage
            .get_credential_store(connection.id())?
            .context("No credentials configured for this connection")?;

        Self::from_credentials(credential_store.as_ref()).await
    }

    /// Sync with storage access for looking up existing accounts.
    pub async fn sync_with_storage<S: Storage + ?Sized>(
        &self,
        connection: &mut Connection,
        storage: &S,
    ) -> Result<SyncResult> {
        self.sync_internal(connection, storage).await
    }
}
