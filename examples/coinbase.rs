//! Coinbase CDP API synchronizer - Proof of Concept
//!
//! This example demonstrates syncing from Coinbase using their CDP API.
//! Credentials are loaded from `pass` (password-store).
//!
//! Run with: cargo run --example coinbase

use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use keepbook::models::{
    Account, Asset, Balance, Connection, ConnectionStatus, Id, LastSync, SyncStatus, Transaction,
};
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::SyncResult;
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use p256::SecretKey;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const CDP_API_BASE: &str = "https://api.coinbase.com";

/// Coinbase CDP API synchronizer
struct CoinbaseSynchronizer {
    key_name: String,
    private_key_pem: String,
    client: Client,
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
    fn new(key_name: String, private_key_pem: String) -> Self {
        Self {
            key_name,
            private_key_pem,
            client: Client::new(),
        }
    }

    /// Load credentials from pass
    fn from_pass() -> Result<Self> {
        let output = std::process::Command::new("pass")
            .arg("show")
            .arg("coinbase-api-key")
            .output()
            .context("Failed to run pass")?;

        if !output.status.success() {
            anyhow::bail!("pass command failed");
        }

        let content =
            String::from_utf8(output.stdout).context("Invalid UTF-8 in pass output")?;

        let mut key_name = None;
        let mut private_key = None;

        for line in content.lines() {
            if let Some(name) = line.strip_prefix("key-name: ") {
                key_name = Some(name.to_string());
            } else if let Some(key) = line.strip_prefix("private-key: ") {
                // The key has literal \n that need to be converted to actual newlines
                private_key = Some(key.replace("\\n", "\n"));
            }
        }

        let key_name = key_name.context("Missing key-name in pass entry")?;
        let private_key = private_key.context("Missing private-key in pass entry")?;

        Ok(Self::new(key_name, private_key))
    }

    fn generate_jwt(&self, method: &str, path: &str) -> Result<String> {
        let now = Utc::now().timestamp();
        let uri = format!(
            "{} {}{}",
            method,
            CDP_API_BASE.replace("https://", "").replace("http://", ""),
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
        let message = format!("{}.{}", header_b64, claims_b64);

        // Parse the EC private key (SEC1 format)
        let secret_key =
            SecretKey::from_sec1_pem(&self.private_key_pem).context("Failed to parse EC private key")?;
        let signing_key = SigningKey::from(&secret_key);

        // Sign the message
        let signature: Signature = signing_key.sign(message.as_bytes());
        let sig_bytes = signature.to_bytes();
        let sig_b64 = URL_SAFE_NO_PAD.encode(&sig_bytes);

        Ok(format!("{}.{}", message, sig_b64))
    }

    async fn request<T: for<'de> Deserialize<'de>>(&self, method: &str, path: &str) -> Result<T> {
        let jwt = self.generate_jwt(method, path)?;
        let url = format!("{}{}", CDP_API_BASE, path);

        let response = self
            .client
            .request(method.parse().unwrap(), &url)
            .header("Authorization", format!("Bearer {}", jwt))
            .header("Content-Type", "application/json")
            .send()
            .await
            .context("HTTP request failed")?;

        let status = response.status();
        let body = response.text().await.context("Failed to read response body")?;

        if !status.is_success() {
            anyhow::bail!("API request failed ({}): {}", status, body);
        }

        serde_json::from_str(&body).context("Failed to parse JSON response")
    }

    async fn get_accounts(&self) -> Result<Vec<CoinbaseAccount>> {
        #[derive(Deserialize)]
        struct Response {
            accounts: Vec<CoinbaseAccount>,
        }

        let resp: Response = self.request("GET", "/api/v3/brokerage/accounts").await?;
        Ok(resp.accounts)
    }

    async fn get_transactions(&self, account_id: &str) -> Result<Vec<CoinbaseTransaction>> {
        #[derive(Deserialize)]
        struct Response {
            #[serde(default)]
            ledger: Vec<CoinbaseTransaction>,
        }

        let path = format!("/api/v3/brokerage/accounts/{}/ledger", account_id);
        let resp: Response = self.request("GET", &path).await?;
        Ok(resp.ledger)
    }

    async fn sync(&self, connection: &mut Connection) -> Result<SyncResult> {
        let coinbase_accounts = self.get_accounts().await?;

        let mut accounts = Vec::new();
        let mut balances: Vec<(Id, Vec<Balance>)> = Vec::new();
        let mut transactions: Vec<(Id, Vec<Transaction>)> = Vec::new();

        for cb_account in coinbase_accounts {
            let account_id = Id::new();
            let asset = Asset::crypto(&cb_account.currency);

            let account = Account::new(cb_account.name.clone(), connection.id.clone());
            let account = Account {
                id: account_id.clone(),
                tags: vec!["coinbase".to_string(), cb_account.account_type.clone()],
                synchronizer_data: serde_json::json!({
                    "coinbase_uuid": cb_account.uuid,
                    "currency": cb_account.currency,
                }),
                ..account
            };

            // Record current balance
            let balance = Balance::new(asset.clone(), &cb_account.available_balance.value);

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
            balances.push((account_id.clone(), vec![balance]));
            transactions.push((account_id, account_transactions));
        }

        // Update connection
        connection.account_ids = accounts.iter().map(|a| a.id.clone()).collect();
        connection.last_sync = Some(LastSync {
            at: Utc::now(),
            status: SyncStatus::Success,
            error: None,
        });
        connection.status = ConnectionStatus::Active;

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

#[tokio::main]
async fn main() -> Result<()> {
    println!("Keepbook - Coinbase Sync (POC)");
    println!("==============================\n");

    let storage = JsonFileStorage::new("data");

    let connections = storage.list_connections().await?;
    let mut connection = connections
        .into_iter()
        .find(|c| c.synchronizer == "coinbase")
        .unwrap_or_else(|| Connection::new("Coinbase", "coinbase"));

    println!("Connection: {} ({})", connection.name, connection.id);

    println!("\nLoading Coinbase credentials from pass...");
    let synchronizer = CoinbaseSynchronizer::from_pass()?;

    println!("Syncing from Coinbase...\n");
    let result = synchronizer.sync(&mut connection).await?;

    for account in &result.accounts {
        println!("  - {} ({})", account.name, account.id);
    }

    for (_account_id, balances) in &result.balances {
        for balance in balances {
            println!(
                "    Balance: {} {}",
                balance.amount,
                serde_json::to_string(&balance.asset)?
            );
        }
    }

    result.save(&storage).await?;

    println!("\nSync complete!");
    println!("Saved {} accounts", result.accounts.len());
    if let Some(last_sync) = &result.connection.last_sync {
        println!("Last sync: {} - {:?}", last_sync.at, last_sync.status);
    }

    Ok(())
}
