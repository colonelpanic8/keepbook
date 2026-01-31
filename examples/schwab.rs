//! Schwab synchronizer - Proof of Concept
//!
//! This example demonstrates syncing from Schwab using scraped data.
//! Since Schwab doesn't have a public API, this uses hardcoded test data
//! that would typically come from browser automation.
//!
//! Run with:
//!   cargo run --example schwab -- setup    # Set up with current scraped data
//!   cargo run --example schwab -- sync     # Sync existing connection

use anyhow::{Context, Result};
use chrono::Utc;
use keepbook::models::{
    Account, Asset, Balance, Connection, ConnectionStatus, Id, LastSync, SyncStatus,
};
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::SyncResult;
use serde::{Deserialize, Serialize};

/// A position/holding in a Schwab account
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SchwabPosition {
    symbol: String,
    description: String,
    quantity: f64,
    price: f64,
    market_value: f64,
    cost_basis: f64,
    gain_loss_dollar: f64,
    gain_loss_percent: f64,
    security_type: String, // "equity", "etf", "cash"
}

/// Scraped account data from Schwab
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SchwabAccountData {
    account_name: String,
    account_number_last4: String,
    account_type: String, // "brokerage", "checking", etc.
    total_value: f64,
    cash_value: f64,
    positions: Vec<SchwabPosition>,
}

/// Schwab synchronizer
struct SchwabSynchronizer;

impl SchwabSynchronizer {
    fn new() -> Self {
        Self
    }

    fn sync_from_scraped_data(
        &self,
        connection: &mut Connection,
        scraped_accounts: Vec<SchwabAccountData>,
    ) -> Result<SyncResult> {
        let mut accounts = Vec::new();
        let mut balances: Vec<(Id, Vec<Balance>)> = Vec::new();

        for scraped in scraped_accounts {
            let account_id = Id::new();

            let account = Account {
                id: account_id.clone(),
                name: scraped.account_name.clone(),
                connection_id: connection.id.clone(),
                tags: vec!["schwab".to_string(), scraped.account_type.clone()],
                created_at: Utc::now(),
                active: true,
                synchronizer_data: serde_json::json!({
                    "account_number_last4": scraped.account_number_last4,
                    "positions": scraped.positions,
                }),
            };

            // Create balance for total account value
            let total_balance = Balance::new(Asset::currency("USD"), scraped.total_value.to_string());

            // Also track individual position values as balances
            let mut account_balances = vec![total_balance];

            // Add individual equity positions as balances
            for position in &scraped.positions {
                if position.security_type != "cash" {
                    let position_balance =
                        Balance::new(Asset::equity(&position.symbol), position.quantity.to_string());
                    account_balances.push(position_balance);
                }
            }

            accounts.push(account);
            balances.push((account_id, account_balances));
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
            transactions: Vec::new(),
        })
    }
}

/// Helper function to create hardcoded test data based on current Schwab positions
fn create_current_schwab_data() -> Vec<SchwabAccountData> {
    vec![
        SchwabAccountData {
            account_name: "Individual Brokerage".to_string(),
            account_number_last4: "739".to_string(),
            account_type: "brokerage".to_string(),
            total_value: 1825933.33,
            cash_value: 6858.05,
            positions: vec![
                SchwabPosition {
                    symbol: "AMT".to_string(),
                    description: "AMERICAN TOWER CORP NEW REIT".to_string(),
                    quantity: 84.0,
                    price: 177.025,
                    market_value: 14870.10,
                    cost_basis: 24897.59,
                    gain_loss_dollar: -10027.49,
                    gain_loss_percent: -40.27,
                    security_type: "equity".to_string(),
                },
                SchwabPosition {
                    symbol: "CHTR".to_string(),
                    description: "CHARTER COMMUNICATIONS CLASS A".to_string(),
                    quantity: 38.0,
                    price: 208.20,
                    market_value: 7911.60,
                    cost_basis: 22503.84,
                    gain_loss_dollar: -14592.24,
                    gain_loss_percent: -64.84,
                    security_type: "equity".to_string(),
                },
                SchwabPosition {
                    symbol: "FG".to_string(),
                    description: "F&G ANNUITIES & LIFE INC".to_string(),
                    quantity: 38.0,
                    price: 28.9523,
                    market_value: 1100.19,
                    cost_basis: 889.93,
                    gain_loss_dollar: 210.26,
                    gain_loss_percent: 23.63,
                    security_type: "equity".to_string(),
                },
                SchwabPosition {
                    symbol: "FNF".to_string(),
                    description: "FNF GROUP CLASS A".to_string(),
                    quantity: 305.0,
                    price: 53.92,
                    market_value: 16445.60,
                    cost_basis: 9985.70,
                    gain_loss_dollar: 6459.90,
                    gain_loss_percent: 64.69,
                    security_type: "equity".to_string(),
                },
                SchwabPosition {
                    symbol: "GOOGL".to_string(),
                    description: "ALPHABET INC CLASS A".to_string(),
                    quantity: 390.2659,
                    price: 338.5729,
                    market_value: 132133.46,
                    cost_basis: 44783.36,
                    gain_loss_dollar: 87350.10,
                    gain_loss_percent: 195.05,
                    security_type: "equity".to_string(),
                },
                SchwabPosition {
                    symbol: "MSFT".to_string(),
                    description: "MICROSOFT CORP".to_string(),
                    quantity: 79.1713,
                    price: 430.4527,
                    market_value: 34079.50,
                    cost_basis: 19602.09,
                    gain_loss_dollar: 14477.41,
                    gain_loss_percent: 73.86,
                    security_type: "equity".to_string(),
                },
                SchwabPosition {
                    symbol: "V".to_string(),
                    description: "VISA INC CLASS A".to_string(),
                    quantity: 223.0,
                    price: 322.5316,
                    market_value: 71924.55,
                    cost_basis: 49066.69,
                    gain_loss_dollar: 22857.86,
                    gain_loss_percent: 46.59,
                    security_type: "equity".to_string(),
                },
                SchwabPosition {
                    symbol: "QQQ".to_string(),
                    description: "INVESCO QQQ ETF".to_string(),
                    quantity: 775.0,
                    price: 625.1576,
                    market_value: 484497.14,
                    cost_basis: 267512.75,
                    gain_loss_dollar: 216984.39,
                    gain_loss_percent: 81.11,
                    security_type: "etf".to_string(),
                },
                SchwabPosition {
                    symbol: "RSP".to_string(),
                    description: "INVESCO S&P 500 EQUAL WEIGHT ETF".to_string(),
                    quantity: 2284.2546,
                    price: 197.13,
                    market_value: 450295.11,
                    cost_basis: 319260.61,
                    gain_loss_dollar: 131034.50,
                    gain_loss_percent: 41.04,
                    security_type: "etf".to_string(),
                },
                SchwabPosition {
                    symbol: "VNQ".to_string(),
                    description: "VANGUARD REAL ESTATE ETF".to_string(),
                    quantity: 931.0,
                    price: 89.88,
                    market_value: 83678.28,
                    cost_basis: 96764.03,
                    gain_loss_dollar: -13085.75,
                    gain_loss_percent: -13.52,
                    security_type: "etf".to_string(),
                },
                SchwabPosition {
                    symbol: "VOO".to_string(),
                    description: "VANGUARD S&P 500 ETF".to_string(),
                    quantity: 602.0,
                    price: 635.9091,
                    market_value: 382817.28,
                    cost_basis: 214920.78,
                    gain_loss_dollar: 167896.50,
                    gain_loss_percent: 78.12,
                    security_type: "etf".to_string(),
                },
                SchwabPosition {
                    symbol: "VXUS".to_string(),
                    description: "VANGUARD TOTAL INTL STOCK ETF".to_string(),
                    quantity: 1585.0124,
                    price: 80.035,
                    market_value: 126856.47,
                    cost_basis: 97079.46,
                    gain_loss_dollar: 29777.01,
                    gain_loss_percent: 30.67,
                    security_type: "etf".to_string(),
                },
                SchwabPosition {
                    symbol: "XBI".to_string(),
                    description: "SPDR S&P BIOTECH ETF".to_string(),
                    quantity: 100.0,
                    price: 124.66,
                    market_value: 12466.00,
                    cost_basis: 9855.64,
                    gain_loss_dollar: 2610.36,
                    gain_loss_percent: 26.49,
                    security_type: "etf".to_string(),
                },
                SchwabPosition {
                    symbol: "CASH".to_string(),
                    description: "Cash & Cash Investments".to_string(),
                    quantity: 6858.05,
                    price: 1.0,
                    market_value: 6858.05,
                    cost_basis: 6858.05,
                    gain_loss_dollar: 0.0,
                    gain_loss_percent: 0.0,
                    security_type: "cash".to_string(),
                },
            ],
        },
        SchwabAccountData {
            account_name: "Investor Checking".to_string(),
            account_number_last4: "420".to_string(),
            account_type: "checking".to_string(),
            total_value: 14487.41,
            cash_value: 14487.41,
            positions: vec![SchwabPosition {
                symbol: "CASH".to_string(),
                description: "Checking Balance".to_string(),
                quantity: 14487.41,
                price: 1.0,
                market_value: 14487.41,
                cost_basis: 14487.41,
                gain_loss_dollar: 0.0,
                gain_loss_percent: 0.0,
                security_type: "cash".to_string(),
            }],
        },
    ]
}

async fn setup(storage: &JsonFileStorage) -> Result<()> {
    let mut connection = Connection::new("Charles Schwab", "schwab");

    println!("Connection: {} ({})", connection.name, connection.id);

    println!("\nUsing current scraped Schwab data...\n");
    let scraped_data = create_current_schwab_data();

    let synchronizer = SchwabSynchronizer::new();
    let result = synchronizer.sync_from_scraped_data(&mut connection, scraped_data)?;

    for account in &result.accounts {
        println!("  - {} ({})", account.name, account.id);
        if let Some(balances) = result.balances.iter().find(|(id, _)| id == &account.id) {
            for balance in &balances.1 {
                println!(
                    "    {} {}",
                    balance.amount,
                    serde_json::to_string(&balance.asset)?
                );
            }
        }
    }

    result.save(storage).await?;

    let total: f64 = create_current_schwab_data()
        .iter()
        .map(|a| a.total_value)
        .sum();

    println!("\nSync complete!");
    println!("Saved {} accounts", result.accounts.len());
    println!("Total value across accounts: ${:.2}", total);

    Ok(())
}

async fn sync(storage: &JsonFileStorage) -> Result<()> {
    let connections = storage.list_connections().await?;
    let connection = connections
        .into_iter()
        .find(|c| c.synchronizer == "schwab");

    let mut connection = connection.context(
        "No Schwab connection found. Run 'cargo run --example schwab -- setup' first.",
    )?;

    println!("Connection: {} ({})", connection.name, connection.id);

    // For now, use the hardcoded data
    // In a full implementation, this would trigger browser automation
    println!("\nUsing current scraped Schwab data...\n");
    let scraped_data = create_current_schwab_data();

    let synchronizer = SchwabSynchronizer::new();
    let result = synchronizer.sync_from_scraped_data(&mut connection, scraped_data)?;

    for account in &result.accounts {
        println!("  - {} ({})", account.name, account.id);
    }

    result.save(storage).await?;

    println!("\nSync complete!");
    println!("Saved {} accounts", result.accounts.len());
    if let Some(last_sync) = &result.connection.last_sync {
        println!("Last sync: {} - {:?}", last_sync.at, last_sync.status);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Keepbook - Schwab Sync (POC)");
    println!("============================\n");

    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("setup");

    let storage = JsonFileStorage::new("data");

    match command {
        "setup" => setup(&storage).await,
        "sync" => sync(&storage).await,
        other => {
            println!("Unknown command: {}", other);
            println!("Usage: cargo run --example schwab -- [setup|sync]");
            Ok(())
        }
    }
}
