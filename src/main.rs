use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use keepbook::config::ResolvedConfig;
use keepbook::market_data::PriceSourceRegistry;
use keepbook::storage::{JsonFileStorage, Storage};
use serde::Serialize;

#[derive(Parser)]
#[command(name = "keepbook")]
#[command(about = "Personal finance manager")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "keepbook.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show current configuration
    Config,

    /// List entities
    #[command(subcommand)]
    List(ListCommand),
}

#[derive(Subcommand)]
enum ListCommand {
    /// List all connections
    Connections,

    /// List all accounts
    Accounts,

    /// List configured price sources
    PriceSources,

    /// List latest balances for all accounts
    Balances,

    /// List all transactions
    Transactions,

    /// List everything
    All,
}

/// JSON output for connections
#[derive(Serialize)]
struct ConnectionOutput {
    id: String,
    name: String,
    synchronizer: String,
    status: String,
    account_count: usize,
    last_sync: Option<String>,
}

/// JSON output for accounts
#[derive(Serialize)]
struct AccountOutput {
    id: String,
    name: String,
    connection_id: String,
    tags: Vec<String>,
    active: bool,
}

/// JSON output for price sources
#[derive(Serialize)]
struct PriceSourceOutput {
    name: String,
    #[serde(rename = "type")]
    source_type: String,
    enabled: bool,
    priority: u32,
    has_credentials: bool,
}

/// JSON output for balances
#[derive(Serialize)]
struct BalanceOutput {
    account_id: String,
    asset: serde_json::Value,
    amount: String,
    timestamp: String,
}

/// JSON output for transactions
#[derive(Serialize)]
struct TransactionOutput {
    id: String,
    account_id: String,
    timestamp: String,
    description: String,
    amount: String,
    asset: serde_json::Value,
    status: String,
}

/// Combined output for list all
#[derive(Serialize)]
struct AllOutput {
    connections: Vec<ConnectionOutput>,
    accounts: Vec<AccountOutput>,
    price_sources: Vec<PriceSourceOutput>,
    balances: Vec<BalanceOutput>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = ResolvedConfig::load_or_default(&cli.config)?;
    let storage = JsonFileStorage::new(&config.data_dir);

    match cli.command {
        Some(Command::Config) => {
            let output = serde_json::json!({
                "config_file": cli.config.display().to_string(),
                "data_directory": config.data_dir.display().to_string(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Some(Command::List(list_cmd)) => match list_cmd {
            ListCommand::Connections => {
                let connections = list_connections(&storage).await?;
                println!("{}", serde_json::to_string_pretty(&connections)?);
            }

            ListCommand::Accounts => {
                let accounts = list_accounts(&storage).await?;
                println!("{}", serde_json::to_string_pretty(&accounts)?);
            }

            ListCommand::PriceSources => {
                let sources = list_price_sources(&config.data_dir)?;
                println!("{}", serde_json::to_string_pretty(&sources)?);
            }

            ListCommand::Balances => {
                let balances = list_balances(&storage).await?;
                println!("{}", serde_json::to_string_pretty(&balances)?);
            }

            ListCommand::Transactions => {
                let transactions = list_transactions(&storage).await?;
                println!("{}", serde_json::to_string_pretty(&transactions)?);
            }

            ListCommand::All => {
                let output = AllOutput {
                    connections: list_connections(&storage).await?,
                    accounts: list_accounts(&storage).await?,
                    price_sources: list_price_sources(&config.data_dir)?,
                    balances: list_balances(&storage).await?,
                };
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
        },

        None => {
            let output = serde_json::json!({
                "name": "keepbook",
                "version": env!("CARGO_PKG_VERSION"),
                "config_file": cli.config.display().to_string(),
                "data_directory": config.data_dir.display().to_string(),
                "commands": ["config", "list connections", "list accounts", "list price-sources", "list balances", "list transactions", "list all"]
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }

    Ok(())
}

async fn list_connections(storage: &JsonFileStorage) -> Result<Vec<ConnectionOutput>> {
    let connections = storage.list_connections().await?;
    Ok(connections
        .into_iter()
        .map(|c| ConnectionOutput {
            id: c.state.id.to_string(),
            name: c.config.name.clone(),
            synchronizer: c.config.synchronizer.clone(),
            status: format!("{:?}", c.state.status).to_lowercase(),
            account_count: c.state.account_ids.len(),
            last_sync: c.state.last_sync.as_ref().map(|ls| ls.at.to_rfc3339()),
        })
        .collect())
}

async fn list_accounts(storage: &JsonFileStorage) -> Result<Vec<AccountOutput>> {
    let accounts = storage.list_accounts().await?;
    Ok(accounts
        .into_iter()
        .map(|a| AccountOutput {
            id: a.id.to_string(),
            name: a.name.clone(),
            connection_id: a.connection_id.to_string(),
            tags: a.tags.clone(),
            active: a.active,
        })
        .collect())
}

fn list_price_sources(data_dir: &std::path::Path) -> Result<Vec<PriceSourceOutput>> {
    let mut registry = PriceSourceRegistry::new(data_dir);
    registry.load()?;

    Ok(registry
        .sources()
        .iter()
        .map(|s| PriceSourceOutput {
            name: s.name.clone(),
            source_type: format!("{:?}", s.config.source_type).to_lowercase(),
            enabled: s.config.enabled,
            priority: s.config.priority,
            has_credentials: s.config.credentials.is_some(),
        })
        .collect())
}

async fn list_balances(storage: &JsonFileStorage) -> Result<Vec<BalanceOutput>> {
    let balances = storage.get_latest_balances().await?;
    Ok(balances
        .into_iter()
        .map(|(account_id, balance)| BalanceOutput {
            account_id: account_id.to_string(),
            asset: serde_json::to_value(&balance.asset).unwrap_or_default(),
            amount: balance.amount.clone(),
            timestamp: balance.timestamp.to_rfc3339(),
        })
        .collect())
}

async fn list_transactions(storage: &JsonFileStorage) -> Result<Vec<TransactionOutput>> {
    let accounts = storage.list_accounts().await?;
    let mut all_transactions = Vec::new();

    for account in accounts {
        let transactions = storage.get_transactions(&account.id).await?;
        for tx in transactions {
            all_transactions.push(TransactionOutput {
                id: tx.id.to_string(),
                account_id: account.id.to_string(),
                timestamp: tx.timestamp.to_rfc3339(),
                description: tx.description.clone(),
                amount: tx.amount.clone(),
                asset: serde_json::to_value(&tx.asset).unwrap_or_default(),
                status: format!("{:?}", tx.status).to_lowercase(),
            });
        }
    }

    Ok(all_transactions)
}
