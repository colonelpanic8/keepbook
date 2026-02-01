use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use keepbook::config::ResolvedConfig;
use keepbook::market_data::PriceSourceRegistry;
use keepbook::models::{Account, Asset, Balance, Connection, ConnectionConfig, ConnectionState, Id};
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

    /// Add entities
    #[command(subcommand)]
    Add(AddCommand),

    /// List entities
    #[command(subcommand)]
    List(ListCommand),

    /// Remove entities
    #[command(subcommand)]
    Remove(RemoveCommand),

    /// Set/update values
    #[command(subcommand)]
    Set(SetCommand),
}

#[derive(Subcommand)]
enum AddCommand {
    /// Add a new manual connection
    Connection {
        /// Name for the connection
        name: String,
    },

    /// Add a new account to a connection
    Account {
        /// Connection ID to add the account to
        #[arg(long)]
        connection: String,

        /// Name for the account
        name: String,

        /// Tags for the account (can be specified multiple times)
        #[arg(long, short)]
        tag: Vec<String>,
    },
}

#[derive(Subcommand)]
enum SetCommand {
    /// Set or update a balance for an account
    Balance {
        /// Account ID
        #[arg(long)]
        account: String,

        /// Asset type (e.g., "USD", "equity:AAPL", "crypto:BTC")
        #[arg(long)]
        asset: String,

        /// Amount
        #[arg(long)]
        amount: String,
    },
}

#[derive(Subcommand)]
enum RemoveCommand {
    /// Remove a connection and all its accounts
    Connection {
        /// Connection ID to remove
        id: String,
    },
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

        Some(Command::Add(add_cmd)) => match add_cmd {
            AddCommand::Connection { name } => {
                let result = add_connection(&storage, &name).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            AddCommand::Account { connection, name, tag } => {
                let result = add_account(&storage, &connection, &name, tag).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Remove(remove_cmd)) => match remove_cmd {
            RemoveCommand::Connection { id } => {
                let result = remove_connection(&storage, &id).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Set(set_cmd)) => match set_cmd {
            SetCommand::Balance { account, asset, amount } => {
                let result = set_balance(&storage, &account, &asset, &amount).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

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
                "commands": ["config", "list connections", "list accounts", "list price-sources", "list balances", "list transactions", "list all", "remove connection <id>"]
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

async fn remove_connection(storage: &JsonFileStorage, id_str: &str) -> Result<serde_json::Value> {
    let id = Id::from_string(id_str);

    // Get connection info first
    let connection = storage.get_connection(&id).await?;
    let conn = match connection {
        Some(c) => c,
        None => {
            return Ok(serde_json::json!({
                "success": false,
                "error": "Connection not found",
                "id": id_str
            }));
        }
    };

    let name = conn.config.name.clone();
    let account_ids: Vec<String> = conn.state.account_ids.iter().map(|a| a.to_string()).collect();

    // Delete all accounts belonging to this connection
    let mut deleted_accounts = 0;
    for account_id in &conn.state.account_ids {
        if storage.delete_account(account_id).await? {
            deleted_accounts += 1;
        }
    }

    // Delete the connection
    storage.delete_connection(&id).await?;

    Ok(serde_json::json!({
        "success": true,
        "connection": {
            "id": id_str,
            "name": name
        },
        "deleted_accounts": deleted_accounts,
        "account_ids": account_ids
    }))
}

async fn add_connection(storage: &JsonFileStorage, name: &str) -> Result<serde_json::Value> {
    let connection = Connection {
        config: ConnectionConfig {
            name: name.to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
        },
        state: ConnectionState::new(),
    };

    let id = connection.state.id.to_string();

    // Save the connection (this creates the directory structure)
    storage.save_connection(&connection).await?;

    // Also write the config TOML since save_connection only writes state
    let config_path = storage.connection_config_path(&connection.state.id);
    let config_toml = toml::to_string_pretty(&connection.config)?;
    tokio::fs::create_dir_all(config_path.parent().unwrap()).await?;
    tokio::fs::write(&config_path, config_toml).await?;

    Ok(serde_json::json!({
        "success": true,
        "connection": {
            "id": id,
            "name": name,
            "synchronizer": "manual"
        }
    }))
}

async fn add_account(
    storage: &JsonFileStorage,
    connection_id: &str,
    name: &str,
    tags: Vec<String>,
) -> Result<serde_json::Value> {
    let conn_id = Id::from_string(connection_id);

    // Verify connection exists
    let mut connection = storage
        .get_connection(&conn_id)
        .await?
        .context("Connection not found")?;

    // Create account
    let account = Account {
        id: Id::new(),
        name: name.to_string(),
        connection_id: conn_id.clone(),
        tags,
        created_at: Utc::now(),
        active: true,
        synchronizer_data: serde_json::Value::Null,
    };

    let account_id = account.id.to_string();

    // Save account
    storage.save_account(&account).await?;

    // Update connection's account_ids
    connection.state.account_ids.push(account.id);
    storage.save_connection(&connection).await?;

    Ok(serde_json::json!({
        "success": true,
        "account": {
            "id": account_id,
            "name": name,
            "connection_id": connection_id
        }
    }))
}

async fn set_balance(
    storage: &JsonFileStorage,
    account_id: &str,
    asset_str: &str,
    amount: &str,
) -> Result<serde_json::Value> {
    let id = Id::from_string(account_id);

    // Verify account exists
    storage
        .get_account(&id)
        .await?
        .context("Account not found")?;

    // Parse asset string (formats: "USD", "equity:AAPL", "crypto:BTC")
    let asset = parse_asset(asset_str)?;

    // Create balance
    let balance = Balance::new(asset.clone(), amount);

    // Append balance
    storage.append_balances(&id, &[balance.clone()]).await?;

    Ok(serde_json::json!({
        "success": true,
        "balance": {
            "account_id": account_id,
            "asset": serde_json::to_value(&asset)?,
            "amount": amount,
            "timestamp": balance.timestamp.to_rfc3339()
        }
    }))
}

/// Parse asset string into Asset enum.
/// Formats: "USD", "EUR" (currency), "equity:AAPL", "crypto:BTC"
fn parse_asset(s: &str) -> Result<Asset> {
    if let Some(symbol) = s.strip_prefix("equity:") {
        Ok(Asset::equity(symbol))
    } else if let Some(symbol) = s.strip_prefix("crypto:") {
        Ok(Asset::crypto(symbol))
    } else {
        // Assume it's a currency code
        Ok(Asset::currency(s))
    }
}
