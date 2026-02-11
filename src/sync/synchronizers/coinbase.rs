//! Coinbase CDP API synchronizer.
//!
//! This synchronizer uses Coinbase's CDP API with JWT authentication.
//! Credentials are loaded via the CredentialStore abstraction.

use std::collections::{HashMap, HashSet};

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
        let uri = format!("{} {}{}", method, base, path);

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

    fn encode_path_segment(value: &str) -> String {
        // RFC 3986 unreserved characters are safe in a path segment.
        // Everything else gets percent-encoded.
        let mut out = String::with_capacity(value.len());
        for b in value.as_bytes() {
            match *b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(*b as char)
                }
                other => out.push_str(&format!("%{other:02X}")),
            }
        }
        out
    }

    fn encode_query_component(value: &str) -> String {
        // RFC 3986 unreserved characters are safe in a query component.
        let mut out = String::with_capacity(value.len());
        for b in value.as_bytes() {
            match *b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(*b as char)
                }
                other => out.push_str(&format!("%{other:02X}")),
            }
        }
        out
    }

    async fn request<T: for<'de> Deserialize<'de>>(&self, method: &str, path: &str) -> Result<T> {
        // Parse/validate the HTTP method up-front so invalid input doesn't panic and we don't
        // do unnecessary JWT work.
        let method: reqwest::Method = method.parse().context("Invalid HTTP method")?;
        let jwt = self.generate_jwt(method.as_str(), path)?;
        let base = self.api_base.trim_end_matches('/');
        let url = format!("{base}{path}");

        let response = self
            .client
            .request(method, &url)
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
            let path = format!(
                "/api/v3/brokerage/portfolios/{}",
                Self::encode_path_segment(&p.uuid)
            );
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

    async fn get_fills(&self) -> Result<Vec<CoinbaseFill>> {
        #[derive(Deserialize)]
        struct Response {
            #[serde(default)]
            fills: Vec<CoinbaseFill>,
            #[serde(default)]
            has_next: bool,
            #[serde(default)]
            cursor: Option<String>,
        }

        let mut fills = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let path = match &cursor {
                Some(c) => format!(
                    "/api/v3/brokerage/orders/historical/fills?cursor={}",
                    Self::encode_query_component(c)
                ),
                None => "/api/v3/brokerage/orders/historical/fills".to_string(),
            };

            let resp: Response = self.request("GET", &path).await?;
            fills.extend(resp.fills);

            if !resp.has_next {
                break;
            }

            cursor = resp.cursor;
            if cursor.is_none() {
                tracing::warn!(
                    "coinbase fills response reported has_next=true without cursor; stopping pagination"
                );
                break;
            }
        }

        Ok(fills)
    }

    fn base_asset_from_product_id(product_id: &str) -> Option<&str> {
        product_id.split('-').next().filter(|s| !s.is_empty())
    }

    async fn sync_internal<S: Storage + ?Sized>(
        &self,
        connection: &mut Connection,
        storage: &S,
    ) -> Result<SyncResult> {
        let coinbase_accounts = self.get_accounts().await?;
        let fills = self.get_fills().await.unwrap_or_else(|e| {
            tracing::warn!(
                error = %e,
                "failed to fetch coinbase fills; continuing with empty transaction set"
            );
            Vec::new()
        });

        // Load existing accounts to check for history
        let existing_accounts = storage.list_accounts().await?;
        let existing_ids: HashSet<Id> = existing_accounts
            .iter()
            .filter(|a| a.connection_id == *connection.id())
            .map(|a| a.id.clone())
            .collect();

        let mut account_uuid_by_currency: HashMap<String, String> = HashMap::new();
        for account in &coinbase_accounts {
            let currency = account.currency.to_uppercase();
            account_uuid_by_currency
                .entry(currency)
                .or_insert_with(|| account.uuid.clone());
        }

        let mut fills_by_account_uuid: HashMap<String, Vec<CoinbaseFill>> = HashMap::new();
        for fill in fills {
            let Some(base_asset) = Self::base_asset_from_product_id(&fill.product_id) else {
                continue;
            };

            let Some(account_uuid) = account_uuid_by_currency.get(&base_asset.to_uppercase())
            else {
                continue;
            };

            fills_by_account_uuid
                .entry(account_uuid.clone())
                .or_default()
                .push(fill);
        }

        let mut accounts = Vec::new();
        let mut balances: Vec<(Id, Vec<SyncedAssetBalance>)> = Vec::new();
        let mut transactions: Vec<(Id, Vec<Transaction>)> = Vec::new();

        for cb_account in coinbase_accounts {
            // Use Coinbase's UUID directly as our account ID
            let account_id = match Id::from_string_checked(&cb_account.uuid) {
                Ok(id) => id,
                // Fall back to a deterministic, filesystem-safe id for weird external values.
                Err(_) => Id::from_external(&format!("coinbase:{}", cb_account.uuid)),
            };
            let asset = Asset::crypto(&cb_account.currency);
            let balance_amount: f64 = cb_account.available_balance.value.parse().unwrap_or(0.0);

            tracing::debug!(
                name = %cb_account.name,
                currency = %cb_account.currency,
                balance = %balance_amount,
                "processing coinbase account"
            );

            // Check if account already exists
            let existing = existing_ids.contains(&account_id);

            let cb_transactions = fills_by_account_uuid
                .remove(&cb_account.uuid)
                .unwrap_or_default();

            // Skip zero-balance accounts unless they already exist or have transactions
            if balance_amount == 0.0 && !existing && cb_transactions.is_empty() {
                continue;
            }

            // Get existing account's created_at or use now
            let created_at = existing_accounts
                .iter()
                .find(|a| a.id == account_id)
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
                    let timestamp = DateTime::parse_from_rfc3339(&tx.trade_time)
                        .ok()?
                        .with_timezone(&Utc);

                    let side = tx.side.trim().to_uppercase();
                    let side_label = if side.is_empty() {
                        "FILL".to_string()
                    } else {
                        side
                    };

                    let mut amount = tx.size.clone();
                    if side_label == "SELL" && !amount.starts_with('-') {
                        amount = format!("-{amount}");
                    }

                    let trade_id = tx.trade_id.clone();
                    let order_id = tx.order_id.clone();
                    let entry_id = tx
                        .entry_id
                        .or(trade_id.clone())
                        .or(order_id.clone())
                        .unwrap_or_else(|| {
                            format!("{}:{}:{}", tx.product_id, tx.trade_time, tx.side)
                        });

                    let tx_id = Id::from_external(&format!("coinbase:fill:{entry_id}"));
                    let description = format!("{} {}", side_label, tx.product_id);

                    Some(
                        Transaction::new(amount, Asset::crypto(&cb_account.currency), description)
                            .with_timestamp(timestamp)
                            .with_id(tx_id)
                            .with_synchronizer_data(serde_json::json!({
                                "coinbase_entry_id": entry_id,
                                "trade_id": trade_id,
                                "order_id": order_id,
                                "product_id": tx.product_id,
                                "side": side_label,
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
struct CoinbaseFill {
    #[serde(default)]
    entry_id: Option<String>,
    #[serde(default)]
    trade_id: Option<String>,
    #[serde(default)]
    order_id: Option<String>,
    product_id: String,
    size: String,
    trade_time: String,
    #[serde(default)]
    side: String,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ConnectionConfig;
    use crate::storage::MemoryStorage;
    use p256::elliptic_curve::rand_core::OsRng;
    use p256::pkcs8::LineEnding;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn request_invalid_http_method_returns_error_not_panic() {
        // The request path validates HTTP method before trying to parse the private key.
        let synchronizer = CoinbaseSynchronizer::new(
            "test-key".to_string(),
            SecretString::new("not a real pem".to_string().into()),
        );

        let err = synchronizer
            // Spaces are not allowed in HTTP method tokens, so parsing must fail.
            .request::<serde_json::Value>("NOT A METHOD", "/api/v3/brokerage/portfolios")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Invalid HTTP method"));
    }

    #[tokio::test]
    async fn sync_works_against_wiremock() -> Result<()> {
        // This is a "real" integration-style unit test: it exercises the actual HTTP code paths,
        // but with a local Wiremock server instead of hitting the network.
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/v3/brokerage/portfolios"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "portfolios": [{"uuid": "p1", "name": "Default"}]
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v3/brokerage/portfolios/p1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "breakdown": {
                    "spot_positions": [{
                        "asset": "BTC",
                        "account_uuid": "11111111-1111-1111-1111-111111111111",
                        "total_balance_crypto": 0.5,
                        "is_cash": false
                    }]
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v3/brokerage/orders/historical/fills"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "fills": [],
                "has_next": false
            })))
            .mount(&server)
            .await;

        // Generate a throwaway P-256 key and encode it in the SEC1 PEM format the sync code expects.
        let secret_key = SecretKey::random(&mut OsRng);
        let pem = secret_key
            .to_sec1_pem(LineEnding::LF)
            .context("Failed to encode test EC private key")?;

        let synchronizer = CoinbaseSynchronizer::new(
            "test-key".to_string(),
            SecretString::new(pem.to_string().into()),
        )
        .with_base_url(server.uri());

        let storage = MemoryStorage::new();
        let mut connection = Connection::new(ConnectionConfig {
            name: "Coinbase".to_string(),
            synchronizer: "coinbase".to_string(),
            credentials: None,
            balance_staleness: None,
        });

        let result = synchronizer.sync(&mut connection, &storage).await?;

        assert_eq!(result.accounts.len(), 1);
        assert_eq!(result.accounts[0].name, "BTC Wallet");
        assert_eq!(result.balances.len(), 1);
        assert_eq!(result.balances[0].1.len(), 1);
        assert!(matches!(
            result.balances[0].1[0].asset_balance.asset,
            Asset::Crypto { .. }
        ));
        assert_eq!(result.balances[0].1[0].asset_balance.amount, "0.5");
        assert_eq!(result.transactions.len(), 1);
        assert!(result.transactions[0].1.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn sync_maps_coinbase_fills_to_wallet_transactions() -> Result<()> {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/v3/brokerage/portfolios"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "portfolios": [{"uuid": "p1", "name": "Default"}]
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v3/brokerage/portfolios/p1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "breakdown": {
                    "spot_positions": [{
                        "asset": "BTC",
                        "account_uuid": "11111111-1111-1111-1111-111111111111",
                        "total_balance_crypto": 1.25,
                        "is_cash": false
                    }]
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v3/brokerage/orders/historical/fills"))
            .and(query_param_is_missing("cursor"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "fills": [{
                    "entry_id": "entry-1",
                    "trade_id": "trade-1",
                    "order_id": "order-1",
                    "product_id": "BTC-USD",
                    "size": "0.10",
                    "trade_time": "2026-02-10T12:34:56Z",
                    "side": "SELL"
                }],
                "has_next": false
            })))
            .mount(&server)
            .await;

        let secret_key = SecretKey::random(&mut OsRng);
        let pem = secret_key
            .to_sec1_pem(LineEnding::LF)
            .context("Failed to encode test EC private key")?;

        let synchronizer = CoinbaseSynchronizer::new(
            "test-key".to_string(),
            SecretString::new(pem.to_string().into()),
        )
        .with_base_url(server.uri());

        let storage = MemoryStorage::new();
        let mut connection = Connection::new(ConnectionConfig {
            name: "Coinbase".to_string(),
            synchronizer: "coinbase".to_string(),
            credentials: None,
            balance_staleness: None,
        });

        let result = synchronizer.sync(&mut connection, &storage).await?;
        let txs = &result.transactions[0].1;
        assert_eq!(txs.len(), 1);
        assert_eq!(txs[0].amount, "-0.10");
        assert_eq!(txs[0].description, "SELL BTC-USD");
        assert_eq!(
            txs[0]
                .synchronizer_data
                .get("coinbase_entry_id")
                .and_then(|v| v.as_str()),
            Some("entry-1")
        );

        Ok(())
    }

    #[tokio::test]
    async fn get_fills_paginates_on_cursor() -> Result<()> {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/v3/brokerage/orders/historical/fills"))
            .and(query_param_is_missing("cursor"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "fills": [{
                    "entry_id": "entry-1",
                    "product_id": "BTC-USD",
                    "size": "0.01",
                    "trade_time": "2026-02-10T00:00:00Z",
                    "side": "BUY"
                }],
                "has_next": true,
                "cursor": "abc123"
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/api/v3/brokerage/orders/historical/fills"))
            .and(query_param("cursor", "abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "fills": [{
                    "entry_id": "entry-2",
                    "product_id": "BTC-USD",
                    "size": "0.02",
                    "trade_time": "2026-02-10T01:00:00Z",
                    "side": "BUY"
                }],
                "has_next": false
            })))
            .expect(1)
            .mount(&server)
            .await;

        let secret_key = SecretKey::random(&mut OsRng);
        let pem = secret_key
            .to_sec1_pem(LineEnding::LF)
            .context("Failed to encode test EC private key")?;

        let synchronizer = CoinbaseSynchronizer::new(
            "test-key".to_string(),
            SecretString::new(pem.to_string().into()),
        )
        .with_base_url(server.uri());

        let fills = synchronizer.get_fills().await?;
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].entry_id.as_deref(), Some("entry-1"));
        assert_eq!(fills[1].entry_id.as_deref(), Some("entry-2"));

        Ok(())
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
