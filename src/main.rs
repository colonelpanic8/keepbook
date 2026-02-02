use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use keepbook::config::ResolvedConfig;
use keepbook::duration::parse_duration;
use keepbook::market_data::{JsonlMarketDataStore, MarketDataStore, PriceSourceRegistry};
use keepbook::models::{Account, Asset, Balance, Connection, ConnectionConfig, ConnectionState, Id};
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::synchronizers::{CoinbaseSynchronizer, SchwabSynchronizer};
use keepbook::sync::{AuthStatus, InteractiveAuth};
use serde::Serialize;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

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

    /// Sync data from connections
    #[command(subcommand)]
    Sync(SyncCommand),

    /// Schwab-specific commands
    #[command(subcommand)]
    Schwab(SchwabCommand),

    /// Portfolio commands
    #[command(subcommand)]
    Portfolio(PortfolioCommand),
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
enum SyncCommand {
    /// Sync a specific connection by ID or name
    Connection {
        /// Connection ID or name
        id_or_name: String,
    },
    /// Sync all connections
    All,
}

#[derive(Subcommand)]
enum SchwabCommand {
    /// Login via browser to capture session
    Login {
        /// Connection ID or name (optional if only one Schwab connection)
        id_or_name: Option<String>,
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

#[derive(Subcommand)]
enum PortfolioCommand {
    /// Calculate portfolio snapshot with valuations
    Snapshot {
        /// Base currency for valuations (default: from config)
        #[arg(long)]
        currency: Option<String>,

        /// Calculate as of this date (YYYY-MM-DD, default: today)
        #[arg(long)]
        date: Option<String>,

        /// Output grouping: asset, account, or both
        #[arg(long, default_value = "both")]
        group_by: String,

        /// Include per-account breakdown when grouping by asset
        #[arg(long)]
        detail: bool,

        /// Fetch prices/FX if stale
        #[arg(long)]
        refresh: bool,

        /// Always fetch fresh prices/FX
        #[arg(long)]
        force_refresh: bool,

        /// Staleness threshold (e.g., 1d, 4h)
        #[arg(long, default_value = "1d")]
        stale_after: String,
    },
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
    // Initialize structured logging to stderr
    // Use RUST_LOG env var for filtering (default: warn)
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")))
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(true)
                .with_level(true)
                .json(),
        )
        .init();

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

        Some(Command::Sync(sync_cmd)) => match sync_cmd {
            SyncCommand::Connection { id_or_name } => {
                let result = sync_connection(&storage, &id_or_name, &config).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SyncCommand::All => {
                let result = sync_all(&storage, &config).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Schwab(schwab_cmd)) => match schwab_cmd {
            SchwabCommand::Login { id_or_name } => {
                let result = schwab_login(&storage, id_or_name.as_deref()).await?;
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

        Some(Command::Portfolio(portfolio_cmd)) => match portfolio_cmd {
            PortfolioCommand::Snapshot {
                currency,
                date,
                group_by,
                detail,
                refresh,
                force_refresh,
                stale_after,
            } => {
                use keepbook::portfolio::{
                    Grouping, PortfolioQuery, PortfolioService, RefreshMode, RefreshPolicy,
                };

                // Parse date
                let as_of_date = match date {
                    Some(d) => chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                        .with_context(|| format!("Invalid date format: {d}"))?,
                    None => chrono::Utc::now().date_naive(),
                };

                // Parse grouping
                let grouping = match group_by.as_str() {
                    "asset" => Grouping::Asset,
                    "account" => Grouping::Account,
                    "both" => Grouping::Both,
                    _ => anyhow::bail!("Invalid grouping: {group_by}. Use: asset, account, both"),
                };

                // Parse stale_after duration
                let stale_threshold = parse_duration(&stale_after)
                    .with_context(|| format!("Invalid duration: {stale_after}"))?;

                // Build refresh policy
                let refresh_mode = if force_refresh {
                    RefreshMode::Force
                } else if refresh {
                    RefreshMode::IfStale
                } else {
                    RefreshMode::CachedOnly
                };

                let refresh_policy = RefreshPolicy {
                    mode: refresh_mode,
                    stale_threshold,
                };

                // Build query
                let query = PortfolioQuery {
                    as_of_date,
                    currency: currency.unwrap_or_else(|| config.reporting_currency.clone()),
                    grouping,
                    include_detail: detail,
                };

                // Setup market data service
                let store = Arc::new(keepbook::market_data::JsonlMarketDataStore::new(
                    &config.data_dir,
                ));

                // Configure price providers if refresh is enabled
                let market_data = if refresh || force_refresh {
                    use keepbook::market_data::{
                        CryptoPriceRouter, EquityPriceRouter, FxRateRouter,
                    };

                    // Load configured price sources from registry
                    let mut registry = PriceSourceRegistry::new(&config.data_dir);
                    registry.load()?;

                    // Build routers from configured sources
                    let equity_sources = registry.build_equity_sources().await?;
                    let crypto_sources = registry.build_crypto_sources().await?;
                    let fx_sources = registry.build_fx_sources().await?;

                    let mut service = keepbook::market_data::MarketDataService::new(store, None);

                    if !equity_sources.is_empty() {
                        let equity_router = EquityPriceRouter::new(equity_sources);
                        service = service.with_equity_router(Arc::new(equity_router));
                    }

                    if !crypto_sources.is_empty() {
                        let crypto_router = CryptoPriceRouter::new(crypto_sources);
                        service = service.with_crypto_router(Arc::new(crypto_router));
                    }

                    if !fx_sources.is_empty() {
                        let fx_router = FxRateRouter::new(fx_sources);
                        service = service.with_fx_router(Arc::new(fx_router));
                    }

                    Arc::new(service)
                } else {
                    Arc::new(keepbook::market_data::MarketDataService::new(store, None))
                };

                let storage_arc: Arc<dyn keepbook::storage::Storage> = Arc::new(storage);
                let service = PortfolioService::new(storage_arc, market_data);

                // Calculate and output
                let snapshot = service.calculate(&query, &refresh_policy).await?;
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            }
        },

        None => {
            let output = serde_json::json!({
                "name": "keepbook",
                "version": env!("CARGO_PKG_VERSION"),
                "config_file": cli.config.display().to_string(),
                "data_directory": config.data_dir.display().to_string(),
                "commands": [
                    "config",
                    "add connection <name>",
                    "add account --connection <id> <name>",
                    "set balance --account <id> --asset <asset> --amount <amount>",
                    "list connections",
                    "list accounts",
                    "list price-sources",
                    "list balances",
                    "list transactions",
                    "list all",
                    "remove connection <id>",
                    "sync connection <id-or-name>",
                    "sync all",
                    "schwab login [id-or-name]",
                    "portfolio snapshot [options]"
                ]
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
            balance_staleness: None,
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

/// Find a connection by ID first, then by name
async fn find_connection(storage: &JsonFileStorage, id_or_name: &str) -> Result<Option<Connection>> {
    // Try by ID first
    let id = Id::from_string(id_or_name);
    if let Some(conn) = storage.get_connection(&id).await? {
        return Ok(Some(conn));
    }

    // Try by name
    let connections = storage.list_connections().await?;
    for conn in connections {
        if conn.config.name.eq_ignore_ascii_case(id_or_name) {
            return Ok(Some(conn));
        }
    }

    Ok(None)
}

/// Sync a specific connection
async fn sync_connection(storage: &JsonFileStorage, id_or_name: &str, config: &ResolvedConfig) -> Result<serde_json::Value> {
    let mut connection = find_connection(storage, id_or_name)
        .await?
        .context(format!("Connection not found: {id_or_name}"))?;

    let conn_name = connection.config.name.clone();
    let conn_id = connection.id().to_string();
    let synchronizer_type = connection.config.synchronizer.clone();

    // Handle auth check for Schwab
    if synchronizer_type == "schwab" {
        let mut synchronizer = SchwabSynchronizer::from_connection(&connection, storage).await?;

        match synchronizer.check_auth().await? {
            AuthStatus::Valid => {}
            AuthStatus::Missing => {
                // Prompt user
                print!("No session found. Run login now? [Y/n] ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                let input = input.trim().to_lowercase();

                if input.is_empty() || input == "y" || input == "yes" {
                    synchronizer.login().await?;
                } else {
                    return Ok(serde_json::json!({
                        "success": false,
                        "error": "No session available",
                        "connection": conn_name
                    }));
                }
            }
            AuthStatus::Expired { reason } => {
                print!("Session expired ({reason}). Run login now? [Y/n] ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                let input = input.trim().to_lowercase();

                if input.is_empty() || input == "y" || input == "yes" {
                    synchronizer.login().await?;
                } else {
                    return Ok(serde_json::json!({
                        "success": false,
                        "error": format!("Session expired: {}", reason),
                        "connection": conn_name
                    }));
                }
            }
        }

        // Now sync
        let result = synchronizer.sync_with_storage(&mut connection, storage).await?;
        result.save(storage).await?;

        // Store prices from sync result
        let prices_stored = store_sync_prices(&result, config).await?;

        return Ok(serde_json::json!({
            "success": true,
            "connection": {
                "id": conn_id,
                "name": conn_name
            },
            "accounts_synced": result.accounts.len(),
            "prices_stored": prices_stored,
            "last_sync": result.connection.state.last_sync.as_ref().map(|ls| ls.at.to_rfc3339())
        }));
    }

    // Handle Coinbase
    if synchronizer_type == "coinbase" {
        let synchronizer = CoinbaseSynchronizer::from_connection(&connection, storage).await?;
        let result = synchronizer.sync_with_storage(&mut connection, storage).await?;
        result.save(storage).await?;

        // Store prices from sync result
        let prices_stored = store_sync_prices(&result, config).await?;

        return Ok(serde_json::json!({
            "success": true,
            "connection": {
                "id": conn_id,
                "name": conn_name
            },
            "accounts_synced": result.accounts.len(),
            "prices_stored": prices_stored,
            "last_sync": result.connection.state.last_sync.as_ref().map(|ls| ls.at.to_rfc3339())
        }));
    }

    Err(anyhow::anyhow!("Unknown synchronizer type: {synchronizer_type}"))
}

/// Store prices from a sync result into the market data store
async fn store_sync_prices(result: &keepbook::sync::SyncResult, config: &ResolvedConfig) -> Result<usize> {
    let market_data_store = JsonlMarketDataStore::new(&config.data_dir);
    let mut count = 0;

    for (_, synced_balances) in &result.balances {
        for sb in synced_balances {
            if let Some(price) = &sb.price {
                market_data_store.put_prices(&[price.clone()]).await?;
                count += 1;
            }
        }
    }

    Ok(count)
}

/// Sync all connections
async fn sync_all(storage: &JsonFileStorage, config: &ResolvedConfig) -> Result<serde_json::Value> {
    let connections = storage.list_connections().await?;

    let mut results = Vec::new();
    for conn in connections {
        let id_or_name = conn.id().to_string();
        match sync_connection(storage, &id_or_name, config).await {
            Ok(result) => results.push(result),
            Err(e) => results.push(serde_json::json!({
                "success": false,
                "connection": conn.config.name,
                "error": e.to_string()
            })),
        }
    }

    Ok(serde_json::json!({
        "results": results,
        "total": results.len()
    }))
}

/// Schwab login command
async fn schwab_login(storage: &JsonFileStorage, id_or_name: Option<&str>) -> Result<serde_json::Value> {
    // Find Schwab connection(s)
    let connections = storage.list_connections().await?;
    let schwab_connections: Vec<_> = connections
        .into_iter()
        .filter(|c| c.config.synchronizer == "schwab")
        .collect();

    let connection = match (id_or_name, schwab_connections.len()) {
        // Explicit ID/name provided
        (Some(id_or_name), _) => {
            find_connection(storage, id_or_name)
                .await?
                .filter(|c| c.config.synchronizer == "schwab")
                .context(format!("Schwab connection not found: {id_or_name}"))?
        }
        // No ID, exactly one Schwab connection
        (None, 1) => schwab_connections.into_iter().next().unwrap(),
        // No ID, no Schwab connections
        (None, 0) => {
            return Err(anyhow::anyhow!("No Schwab connections found"));
        }
        // No ID, multiple Schwab connections
        (None, n) => {
            let names: Vec<_> = schwab_connections.iter().map(|c| &c.config.name).collect();
            return Err(anyhow::anyhow!(
                "Multiple Schwab connections found ({n}). Specify one: {names:?}"
            ));
        }
    };

    let conn_name = connection.config.name.clone();
    let conn_id = connection.id().to_string();

    let mut synchronizer = SchwabSynchronizer::from_connection(&connection, storage).await?;
    synchronizer.login().await?;

    Ok(serde_json::json!({
        "success": true,
        "connection": {
            "id": conn_id,
            "name": conn_name
        },
        "message": "Session captured successfully"
    }))
}

