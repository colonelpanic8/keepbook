//! Plaid API synchronizer - Proof of Concept
//!
//! This example demonstrates syncing from Plaid.
//! Credentials are loaded from `pass` (password-store).
//!
//! Run with:
//!   cargo run --example plaid -- setup    # Set up a new sandbox connection
//!   cargo run --example plaid -- sync     # Sync existing connection

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use keepbook::models::{
    Account, Asset, AssetBalance, Connection, ConnectionConfig, ConnectionStatus, Id, LastSync, SyncStatus, Transaction,
    TransactionStatus,
};
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::{SyncedAssetBalance, SyncResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum PlaidEnvironment {
    Sandbox,
    Production,
}

impl PlaidEnvironment {
    fn base_url(&self) -> &'static str {
        match self {
            PlaidEnvironment::Sandbox => "https://sandbox.plaid.com",
            PlaidEnvironment::Production => "https://production.plaid.com",
        }
    }
}

/// Plaid API synchronizer
struct PlaidSynchronizer {
    client_id: String,
    secret: String,
    base_url: String,
    client: Client,
}

impl PlaidSynchronizer {
    fn new(client_id: String, secret: String, environment: PlaidEnvironment) -> Self {
        Self {
            client_id,
            secret,
            base_url: environment.base_url().to_string(),
            client: Client::new(),
        }
    }

    /// Load credentials from pass
    fn from_pass() -> Result<Self> {
        let output = std::process::Command::new("pass")
            .arg("show")
            .arg("plaid-api-key")
            .output()
            .context("Failed to run pass")?;

        if !output.status.success() {
            anyhow::bail!("pass command failed");
        }

        let content =
            String::from_utf8(output.stdout).context("Invalid UTF-8 in pass output")?;

        let mut client_id = None;
        let mut secret = None;
        let mut environment = PlaidEnvironment::Sandbox;

        for line in content.lines() {
            if let Some(id) = line.strip_prefix("client-id: ") {
                client_id = Some(id.to_string());
            } else if let Some(s) = line.strip_prefix("secret: ") {
                secret = Some(s.to_string());
            } else if let Some(env) = line.strip_prefix("environment: ") {
                environment = match env {
                    "production" => PlaidEnvironment::Production,
                    _ => PlaidEnvironment::Sandbox,
                };
            }
        }

        let client_id = client_id.context("Missing client-id in pass entry")?;
        let secret = secret.context("Missing secret in pass entry")?;

        Ok(Self::new(client_id, secret, environment))
    }

    async fn request<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .context("HTTP request failed")?;

        let status = response.status();
        let body_text = response.text().await.context("Failed to read response body")?;

        if !status.is_success() {
            anyhow::bail!("Plaid API request failed ({status}): {body_text}");
        }

        serde_json::from_str(&body_text).context("Failed to parse JSON response")
    }

    /// Create a sandbox public token for testing
    async fn create_sandbox_public_token(
        &self,
        institution_id: &str,
        products: &[&str],
    ) -> Result<String> {
        #[derive(Serialize)]
        struct Request<'a> {
            client_id: &'a str,
            secret: &'a str,
            institution_id: &'a str,
            initial_products: &'a [&'a str],
        }

        #[derive(Deserialize)]
        struct Response {
            public_token: String,
        }

        let resp: Response = self
            .request(
                "/sandbox/public_token/create",
                &Request {
                    client_id: &self.client_id,
                    secret: &self.secret,
                    institution_id,
                    initial_products: products,
                },
            )
            .await?;

        Ok(resp.public_token)
    }

    /// Exchange a public token for an access token
    async fn exchange_public_token(&self, public_token: &str) -> Result<String> {
        #[derive(Serialize)]
        struct Request<'a> {
            client_id: &'a str,
            secret: &'a str,
            public_token: &'a str,
        }

        #[derive(Deserialize)]
        struct Response {
            access_token: String,
            #[allow(dead_code)]
            item_id: String,
        }

        let resp: Response = self
            .request(
                "/item/public_token/exchange",
                &Request {
                    client_id: &self.client_id,
                    secret: &self.secret,
                    public_token,
                },
            )
            .await?;

        Ok(resp.access_token)
    }

    /// Get balances for accounts
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

        let resp: Response = self
            .request(
                "/accounts/balance/get",
                &Request {
                    client_id: &self.client_id,
                    secret: &self.secret,
                    access_token,
                },
            )
            .await?;

        Ok(resp.accounts)
    }

    /// Get transactions for a date range
    async fn get_transactions(
        &self,
        access_token: &str,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<PlaidTransaction>> {
        #[derive(Serialize)]
        struct Request<'a> {
            client_id: &'a str,
            secret: &'a str,
            access_token: &'a str,
            start_date: String,
            end_date: String,
        }

        #[derive(Deserialize)]
        struct Response {
            transactions: Vec<PlaidTransaction>,
        }

        let resp: Response = self
            .request(
                "/transactions/get",
                &Request {
                    client_id: &self.client_id,
                    secret: &self.secret,
                    access_token,
                    start_date: start_date.format("%Y-%m-%d").to_string(),
                    end_date: end_date.format("%Y-%m-%d").to_string(),
                },
            )
            .await?;

        Ok(resp.transactions)
    }

    async fn sync(&self, connection: &mut Connection) -> Result<SyncResult> {
        let access_token = connection
            .state
            .synchronizer_data
            .get("access_token")
            .and_then(|v| v.as_str())
            .context("No access_token in connection synchronizer_data")?
            .to_string();

        // Get accounts with balances
        let plaid_accounts = self.get_balances(&access_token).await?;

        // Get transactions for the last 30 days
        let end_date = Utc::now().date_naive();
        let start_date = end_date - chrono::Duration::days(30);
        let plaid_transactions = self
            .get_transactions(&access_token, start_date, end_date)
            .await
            .unwrap_or_else(|e| {
                eprintln!("Warning: Failed to get transactions: {e}");
                Vec::new()
            });

        let mut accounts = Vec::new();
        let mut balances: Vec<(Id, Vec<SyncedAssetBalance>)> = Vec::new();
        let mut transactions: Vec<(Id, Vec<Transaction>)> = Vec::new();

        // Build a map of account_id -> transactions
        let mut tx_by_account: std::collections::HashMap<String, Vec<&PlaidTransaction>> =
            std::collections::HashMap::new();
        for tx in &plaid_transactions {
            tx_by_account
                .entry(tx.account_id.clone())
                .or_default()
                .push(tx);
        }

        for plaid_account in plaid_accounts {
            let account_id = Id::new();

            // Determine asset type based on account type
            let asset = match plaid_account.account_type.as_str() {
                "investment" | "brokerage" => Asset::currency("USD"),
                _ => Asset::currency(
                    plaid_account
                        .balances
                        .iso_currency_code
                        .as_deref()
                        .unwrap_or("USD"),
                ),
            };

            let account = Account {
                id: account_id.clone(),
                name: plaid_account.name.clone(),
                connection_id: connection.id().clone(),
                tags: vec![
                    "plaid".to_string(),
                    plaid_account.account_type.clone(),
                    plaid_account.subtype.clone().unwrap_or_default(),
                ],
                created_at: Utc::now(),
                active: true,
                synchronizer_data: serde_json::json!({
                    "plaid_account_id": plaid_account.account_id,
                    "mask": plaid_account.mask,
                    "official_name": plaid_account.official_name,
                }),
            };

            // Record current balance
            let asset_balance = AssetBalance::new(
                asset.clone(),
                plaid_account
                    .balances
                    .current
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "0".to_string()),
            );

            // Convert Plaid transactions to our format
            let account_transactions: Vec<Transaction> = tx_by_account
                .get(&plaid_account.account_id)
                .map(|txs| {
                    txs.iter()
                        .filter_map(|tx| {
                            let timestamp = NaiveDate::parse_from_str(&tx.date, "%Y-%m-%d")
                                .ok()?
                                .and_hms_opt(12, 0, 0)?;
                            let timestamp = DateTime::from_naive_utc_and_offset(timestamp, Utc);

                            Some(
                                Transaction::new(
                                    (-tx.amount).to_string(),
                                    Asset::currency(tx.iso_currency_code.as_deref().unwrap_or("USD")),
                                    &tx.name,
                                )
                                .with_timestamp(timestamp)
                                .with_status(if tx.pending {
                                    TransactionStatus::Pending
                                } else {
                                    TransactionStatus::Posted
                                })
                                .with_synchronizer_data(serde_json::json!({
                                    "plaid_transaction_id": tx.transaction_id,
                                    "category": tx.category,
                                    "merchant_name": tx.merchant_name,
                                })),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();

            accounts.push(account);
            balances.push((account_id.clone(), vec![SyncedAssetBalance::new(asset_balance)]));
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

#[derive(Debug, Deserialize)]
struct PlaidTransaction {
    transaction_id: String,
    account_id: String,
    amount: f64,
    date: String,
    name: String,
    pending: bool,
    category: Option<Vec<String>>,
    merchant_name: Option<String>,
    iso_currency_code: Option<String>,
}

async fn setup(storage: &JsonFileStorage, synchronizer: &PlaidSynchronizer) -> Result<()> {
    // For sandbox, we create a test institution connection
    let institution_id = "ins_109508";
    let products = &["transactions", "auth"];

    println!("Creating sandbox public token for institution {institution_id}...");
    let public_token = synchronizer
        .create_sandbox_public_token(institution_id, products)
        .await?;

    println!("Exchanging public token for access token...");
    let access_token = synchronizer.exchange_public_token(&public_token).await?;

    println!("Access token obtained successfully!\n");

    // Create a new connection with the access token
    let mut connection = Connection::new(ConnectionConfig {
        name: "Plaid Sandbox".to_string(),
        synchronizer: "plaid".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    connection.state.synchronizer_data = serde_json::json!({
        "access_token": access_token,
        "institution_id": institution_id,
        "environment": "sandbox",
    });

    println!("Connection: {} ({})", connection.name(), connection.id());

    println!("\nPerforming initial sync...\n");
    let result = synchronizer.sync(&mut connection).await?;

    for account in &result.accounts {
        println!("  - {} ({})", account.name, account.id);
    }

    result.save(storage).await?;

    println!("\nSync complete!");
    println!("Saved {} accounts", result.accounts.len());
    println!("\nPlaid setup complete! You can now run 'cargo run --example plaid -- sync' to sync.");

    Ok(())
}

async fn sync(storage: &JsonFileStorage, synchronizer: &PlaidSynchronizer) -> Result<()> {
    let connections = storage.list_connections().await?;
    let connection = connections
        .into_iter()
        .find(|c| c.synchronizer() == "plaid");

    let mut connection = connection.context(
        "No Plaid connection found. Run 'cargo run --example plaid -- setup' first.",
    )?;

    println!("Connection: {} ({})", connection.name(), connection.id());

    println!("\nSyncing from Plaid...\n");
    let result = synchronizer.sync(&mut connection).await?;

    for account in &result.accounts {
        println!("  - {} ({})", account.name, account.id);
    }

    result.save(storage).await?;

    println!("\nSync complete!");
    println!("Saved {} accounts", result.accounts.len());
    if let Some(last_sync) = &result.connection.state.last_sync {
        println!("Last sync: {} - {:?}", last_sync.at, last_sync.status);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Keepbook - Plaid Sync (POC)");
    println!("===========================\n");

    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("sync");

    let storage = JsonFileStorage::new("data");

    println!("Loading Plaid credentials from pass...");
    let synchronizer = PlaidSynchronizer::from_pass()?;

    match command {
        "setup" => setup(&storage, &synchronizer).await,
        "sync" => sync(&storage, &synchronizer).await,
        other => {
            println!("Unknown command: {other}");
            println!("Usage: cargo run --example plaid -- [setup|sync]");
            Ok(())
        }
    }
}
