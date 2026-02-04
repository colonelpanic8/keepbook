use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use clap::{CommandFactory, Parser, Subcommand};
use keepbook::config::{default_config_path, ResolvedConfig};
use keepbook::market_data::{
    FxRateKind, FxRatePoint, JsonlMarketDataStore, MarketDataService, MarketDataStore, PriceKind,
    PricePoint, PriceSourceRegistry,
};
use keepbook::models::{
    Account, Asset, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig, ConnectionState,
    Id,
};
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
    #[arg(short, long, default_value_os_t = default_config_path())]
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

    /// Authentication commands for synchronizers
    #[command(subcommand)]
    Auth(AuthCommand),

    /// Market data commands
    #[command(subcommand)]
    MarketData(MarketDataCommand),

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
        /// Only sync if data is stale
        #[arg(long)]
        if_stale: bool,
    },
    /// Sync all connections
    All {
        /// Only sync connections with stale data
        #[arg(long)]
        if_stale: bool,
    },
    /// Rebuild all symlinks (connections/by-name and account directories)
    Symlinks,
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Schwab authentication commands
    #[command(subcommand)]
    Schwab(SchwabAuthCommand),
}

#[derive(Subcommand)]
enum SchwabAuthCommand {
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
enum MarketDataCommand {
    /// Fetch historical prices for assets in scope
    Fetch {
        /// Account ID or name (mutually exclusive with --connection)
        #[arg(long)]
        account: Option<String>,

        /// Connection ID or name (mutually exclusive with --account)
        #[arg(long)]
        connection: Option<String>,

        /// Start date (YYYY-MM-DD, default: earliest balance date in scope)
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD, default: today)
        #[arg(long)]
        end: Option<String>,

        /// Interval for backfill: daily, weekly, monthly, yearly (default: monthly)
        #[arg(long, default_value = "monthly")]
        interval: String,

        /// Base currency for FX rates (default: from config)
        #[arg(long)]
        currency: Option<String>,

        /// Disable FX rate fetching
        #[arg(long)]
        no_fx: bool,
    },
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

        /// Auto-refresh stale data (default behavior, explicit flag for scripts)
        #[arg(long, conflicts_with_all = ["offline", "dry_run", "force_refresh"])]
        auto: bool,

        /// Use cached data only, no network requests
        #[arg(long, conflicts_with_all = ["auto", "dry_run", "force_refresh"])]
        offline: bool,

        /// Show what would be refreshed without actually refreshing
        #[arg(long, conflicts_with_all = ["auto", "offline", "force_refresh"])]
        dry_run: bool,

        /// Force refresh all data regardless of staleness
        #[arg(long, conflicts_with_all = ["auto", "offline", "dry_run"])]
        force_refresh: bool,
    },

    /// Track net worth over time at every change point
    History {
        /// Base currency for valuations (default: from config)
        #[arg(long)]
        currency: Option<String>,

        /// Start date for history (YYYY-MM-DD, default: earliest data)
        #[arg(long)]
        start: Option<String>,

        /// End date for history (YYYY-MM-DD, default: today)
        #[arg(long)]
        end: Option<String>,

        /// Time granularity: none/full, hourly, daily, weekly, monthly, yearly (default: none)
        #[arg(long, default_value = "none")]
        granularity: String,

        /// Include price changes as change points (slower, more detailed)
        #[arg(long)]
        include_prices: bool,
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

/// A single point in the net worth history
#[derive(Serialize)]
struct HistoryPoint {
    timestamp: String,
    date: String,
    total_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    change_triggers: Option<Vec<String>>,
}

/// Output for portfolio history command
#[derive(Serialize)]
struct HistoryOutput {
    currency: String,
    start_date: Option<String>,
    end_date: Option<String>,
    granularity: String,
    points: Vec<HistoryPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<HistorySummary>,
}

/// Summary statistics for the history
#[derive(Serialize)]
struct HistorySummary {
    initial_value: String,
    final_value: String,
    absolute_change: String,
    percentage_change: String,
}

/// Scope output for market data history fetch
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PriceHistoryScopeOutput {
    Portfolio,
    Connection { id: String, name: String },
    Account { id: String, name: String },
}

/// Asset info output for market data history fetch
#[derive(Serialize)]
struct AssetInfoOutput {
    asset: Asset,
    asset_id: String,
}

/// Summary stats for market data history fetch
#[derive(Default, Serialize)]
struct PriceHistoryStats {
    attempted: usize,
    existing: usize,
    fetched: usize,
    lookback: usize,
    missing: usize,
}

/// Failure details for market data history fetch (sampled)
#[derive(Serialize)]
struct PriceHistoryFailure {
    kind: String,
    date: String,
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    asset: Option<Asset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quote: Option<String>,
}

/// Output for market data history fetch
#[derive(Serialize)]
struct PriceHistoryOutput {
    scope: PriceHistoryScopeOutput,
    currency: String,
    interval: String,
    start_date: String,
    end_date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    earliest_balance_date: Option<String>,
    days: usize,
    points: usize,
    assets: Vec<AssetInfoOutput>,
    prices: PriceHistoryStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    fx: Option<PriceHistoryStats>,
    failure_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    failures: Vec<PriceHistoryFailure>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging to stderr
    // Use RUST_LOG env var for filtering (default: info, suppress noisy chromiumoxide errors)
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(
                "info,chromiumoxide=warn,chromiumoxide::conn=off,chromiumoxide::handler=off",
            )
        }))
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
            AddCommand::Account {
                connection,
                name,
                tag,
            } => {
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
            SetCommand::Balance {
                account,
                asset,
                amount,
            } => {
                let result = set_balance(&storage, &account, &asset, &amount).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Sync(sync_cmd)) => match sync_cmd {
            SyncCommand::Connection {
                id_or_name,
                if_stale,
            } => {
                if if_stale {
                    use keepbook::staleness::{check_balance_staleness, resolve_balance_staleness};

                    let connection = find_connection(&storage, &id_or_name)
                        .await?
                        .context(format!("Connection not found: {id_or_name}"))?;

                    let threshold = resolve_balance_staleness(None, &connection, &config.refresh);
                    let check = check_balance_staleness(&connection, threshold);

                    if !check.is_stale {
                        let output = serde_json::json!({
                            "success": true,
                            "skipped": true,
                            "reason": "not stale",
                            "connection": connection.config.name
                        });
                        println!("{}", serde_json::to_string_pretty(&output)?);
                        return Ok(());
                    }
                }

                let result = sync_connection(&storage, &id_or_name, &config).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SyncCommand::All { if_stale } => {
                if if_stale {
                    use keepbook::staleness::{check_balance_staleness, resolve_balance_staleness};

                    let connections = storage.list_connections().await?;
                    let mut results = Vec::new();

                    for connection in connections {
                        let threshold =
                            resolve_balance_staleness(None, &connection, &config.refresh);
                        let check = check_balance_staleness(&connection, threshold);

                        if check.is_stale {
                            // Sync this connection
                            let id_or_name = connection.id().to_string();
                            match sync_connection(&storage, &id_or_name, &config).await {
                                Ok(result) => results.push(result),
                                Err(e) => results.push(serde_json::json!({
                                    "success": false,
                                    "connection": connection.config.name,
                                    "error": e.to_string()
                                })),
                            }
                        } else {
                            // Skip this connection
                            results.push(serde_json::json!({
                                "success": true,
                                "skipped": true,
                                "reason": "not stale",
                                "connection": connection.config.name
                            }));
                        }
                    }

                    let output = serde_json::json!({
                        "results": results,
                        "total": results.len()
                    });
                    println!("{}", serde_json::to_string_pretty(&output)?);
                } else {
                    let result = sync_all(&storage, &config).await?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
            }
            SyncCommand::Symlinks => {
                let (conn_created, acct_created, warnings) = storage.rebuild_all_symlinks().await?;
                for warning in &warnings {
                    eprintln!("Warning: {warning}");
                }
                let result = serde_json::json!({
                    "connection_symlinks_created": conn_created,
                    "account_symlinks_created": acct_created,
                    "warnings": warnings.len()
                });
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Auth(auth_cmd)) => match auth_cmd {
            AuthCommand::Schwab(schwab_cmd) => match schwab_cmd {
                SchwabAuthCommand::Login { id_or_name } => {
                    let result = schwab_login(&storage, id_or_name.as_deref()).await?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
            },
        },

        Some(Command::MarketData(market_cmd)) => match market_cmd {
            MarketDataCommand::Fetch {
                account,
                connection,
                start,
                end,
                interval,
                currency,
                no_fx,
            } => {
                let output = fetch_historical_prices(PriceHistoryRequest {
                    storage: &storage,
                    config: &config,
                    account: account.as_deref(),
                    connection: connection.as_deref(),
                    start: start.as_deref(),
                    end: end.as_deref(),
                    interval: interval.as_str(),
                    currency,
                    include_fx: !no_fx,
                })
                .await?;
                println!("{}", serde_json::to_string_pretty(&output)?);
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
                auto,
                offline,
                dry_run,
                force_refresh,
            } => {
                use keepbook::portfolio::{Grouping, PortfolioQuery, PortfolioService};
                use keepbook::staleness::{
                    check_balance_staleness, check_price_staleness, log_balance_staleness,
                    log_price_staleness, resolve_balance_staleness,
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

                // Determine what to refresh based on flags
                // Default (no flags or --auto): auto-refresh stale data
                // --offline: no refresh
                // --dry-run: log staleness but no refresh
                // --force-refresh: refresh everything
                let should_refresh_balances = !offline && !dry_run;
                let should_refresh_prices = !offline && !dry_run;
                let ignore_staleness = force_refresh;

                // Explicit --auto flag has same behavior as default
                let _ = auto;

                // Build query
                let query = PortfolioQuery {
                    as_of_date,
                    currency: currency.unwrap_or_else(|| config.reporting_currency.clone()),
                    grouping,
                    include_detail: detail,
                };

                // Setup market data store
                let store = Arc::new(keepbook::market_data::JsonlMarketDataStore::new(
                    &config.data_dir,
                ));

                // Check which connections need syncing based on staleness
                let connections = storage.list_connections().await?;
                let mut connections_to_sync = Vec::new();

                for connection in &connections {
                    let threshold = resolve_balance_staleness(None, connection, &config.refresh);
                    let check = check_balance_staleness(connection, threshold);

                    // Log if dry_run
                    if dry_run {
                        log_balance_staleness(&connection.config.name, &check);
                    }

                    // Add to sync list if stale (or force)
                    if should_refresh_balances && (ignore_staleness || check.is_stale) {
                        connections_to_sync.push(connection.clone());
                    }
                }

                // Check price staleness for dry-run
                if dry_run {
                    use keepbook::market_data::{AssetId, MarketDataStore, PriceKind};
                    use std::collections::HashSet;

                    // Load balances to find unique assets that need prices
                    let snapshots = storage.get_latest_balances().await?;
                    let mut seen_assets: HashSet<String> = HashSet::new();

                    for (_, snapshot) in &snapshots {
                        for asset_balance in &snapshot.balances {
                            match &asset_balance.asset {
                                keepbook::models::Asset::Equity { .. }
                                | keepbook::models::Asset::Crypto { .. } => {
                                    let asset_id = AssetId::from_asset(&asset_balance.asset);
                                    let asset_key = asset_id.to_string();

                                    if seen_assets.contains(&asset_key) {
                                        continue;
                                    }
                                    seen_assets.insert(asset_key.clone());

                                    // Find most recent cached price (quote or close, with lookback)
                                    let mut cached_price = None;

                                    // Try Quote for today first
                                    if let Some(p) = store
                                        .get_price(&asset_id, query.as_of_date, PriceKind::Quote)
                                        .await?
                                    {
                                        cached_price = Some(p);
                                    }

                                    // If no quote, try Close with lookback (7 days)
                                    if cached_price.is_none() {
                                        for offset in 0..=7i64 {
                                            let target_date =
                                                query.as_of_date - chrono::Duration::days(offset);
                                            if let Some(p) = store
                                                .get_price(&asset_id, target_date, PriceKind::Close)
                                                .await?
                                            {
                                                cached_price = Some(p);
                                                break;
                                            }
                                        }
                                    }

                                    let check = check_price_staleness(
                                        cached_price.as_ref(),
                                        config.refresh.price_staleness,
                                    );
                                    log_price_staleness(&asset_key, &check);
                                }
                                keepbook::models::Asset::Currency { .. } => {
                                    // Currency doesn't need price lookup (only FX)
                                }
                            }
                        }
                    }
                }

                // Sync stale connections
                if !connections_to_sync.is_empty() {
                    for connection in &connections_to_sync {
                        let _ = sync_connection(&storage, connection.id().as_ref(), &config).await;
                    }
                }

                // Setup market data service with or without providers
                let market_data = if should_refresh_prices {
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

                    let mut service = keepbook::market_data::MarketDataService::new(store, None)
                        .with_quote_staleness(config.refresh.price_staleness);

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

                // Calculate and output
                let storage_arc: Arc<dyn keepbook::storage::Storage> = Arc::new(storage);
                let service = PortfolioService::new(storage_arc, market_data);
                let snapshot = service.calculate(&query).await?;
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            }

            PortfolioCommand::History {
                currency,
                start,
                end,
                granularity,
                include_prices,
            } => {
                use keepbook::market_data::MarketDataStore;
                use keepbook::portfolio::{
                    collect_change_points, filter_by_date_range, filter_by_granularity,
                    CoalesceStrategy, CollectOptions, Granularity, PortfolioQuery,
                    PortfolioService,
                };
                use rust_decimal::Decimal;
                use std::str::FromStr;

                // Parse date range
                let start_date = start
                    .as_ref()
                    .map(|s| {
                        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                            .with_context(|| format!("Invalid start date: {s}"))
                    })
                    .transpose()?;
                let end_date = end
                    .as_ref()
                    .map(|s| {
                        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                            .with_context(|| format!("Invalid end date: {s}"))
                    })
                    .transpose()?;

                // Parse granularity
                let granularity_enum = match granularity.as_str() {
                    "none" | "full" => Granularity::Full,
                    "hourly" => Granularity::Hourly,
                    "daily" => Granularity::Daily,
                    "weekly" => Granularity::Weekly,
                    "monthly" => Granularity::Monthly,
                    "yearly" => Granularity::Yearly,
                    _ => anyhow::bail!(
                        "Invalid granularity: {granularity}. Use: none, full, hourly, daily, weekly, monthly, yearly"
                    ),
                };

                // Setup storage and market data store
                let store: Arc<dyn MarketDataStore> = Arc::new(
                    keepbook::market_data::JsonlMarketDataStore::new(&config.data_dir),
                );
                let storage_arc: Arc<dyn keepbook::storage::Storage> = Arc::new(storage);

                // Collect change points
                let options = CollectOptions {
                    account_ids: Vec::new(), // All accounts
                    include_prices,
                    include_fx: false,
                    target_currency: currency.clone(),
                };

                let change_points = collect_change_points(&storage_arc, &store, &options).await?;

                // Filter by date range
                let filtered_by_date = filter_by_date_range(change_points, start_date, end_date);

                // Filter by granularity
                let filtered = filter_by_granularity(
                    filtered_by_date,
                    granularity_enum,
                    CoalesceStrategy::Last,
                );

                if filtered.is_empty() {
                    let output = HistoryOutput {
                        currency: currency.unwrap_or_else(|| config.reporting_currency.clone()),
                        start_date: start,
                        end_date: end,
                        granularity,
                        points: Vec::new(),
                        summary: None,
                    };
                    println!("{}", serde_json::to_string_pretty(&output)?);
                    return Ok(());
                }

                // Setup market data service (offline mode - use cached data only)
                let market_data =
                    Arc::new(keepbook::market_data::MarketDataService::new(store, None));

                // Create portfolio service
                let service = PortfolioService::new(storage_arc, market_data);

                // Calculate portfolio value at each change point
                let target_currency = currency
                    .clone()
                    .unwrap_or_else(|| config.reporting_currency.clone());
                let mut history_points = Vec::with_capacity(filtered.len());

                for change_point in &filtered {
                    let as_of_date = change_point.timestamp.date_naive();
                    let query = PortfolioQuery {
                        as_of_date,
                        currency: target_currency.clone(),
                        grouping: keepbook::portfolio::Grouping::Asset,
                        include_detail: false,
                    };

                    let snapshot = service.calculate(&query).await?;

                    // Format trigger descriptions
                    let trigger_descriptions: Vec<String> = change_point
                        .triggers
                        .iter()
                        .map(|t| match t {
                            keepbook::portfolio::ChangeTrigger::Balance { account_id, asset } => {
                                format!(
                                    "balance:{}:{}",
                                    account_id,
                                    serde_json::to_string(asset).unwrap_or_default()
                                )
                            }
                            keepbook::portfolio::ChangeTrigger::Price { asset_id } => {
                                format!("price:{asset_id}")
                            }
                            keepbook::portfolio::ChangeTrigger::FxRate { base, quote } => {
                                format!("fx:{base}/{quote}")
                            }
                        })
                        .collect();

                    history_points.push(HistoryPoint {
                        timestamp: change_point.timestamp.to_rfc3339(),
                        date: as_of_date.to_string(),
                        total_value: snapshot.total_value,
                        change_triggers: if trigger_descriptions.is_empty() {
                            None
                        } else {
                            Some(trigger_descriptions)
                        },
                    });
                }

                // Calculate summary if we have points
                let summary = if history_points.len() >= 2 {
                    let initial =
                        Decimal::from_str(&history_points[0].total_value).unwrap_or(Decimal::ZERO);
                    let final_val =
                        Decimal::from_str(&history_points[history_points.len() - 1].total_value)
                            .unwrap_or(Decimal::ZERO);
                    let absolute_change = final_val - initial;
                    let percentage_change = if initial != Decimal::ZERO {
                        ((final_val - initial) / initial * Decimal::from(100))
                            .round_dp(2)
                            .to_string()
                    } else {
                        "N/A".to_string()
                    };

                    Some(HistorySummary {
                        initial_value: initial.normalize().to_string(),
                        final_value: final_val.normalize().to_string(),
                        absolute_change: absolute_change.normalize().to_string(),
                        percentage_change,
                    })
                } else {
                    None
                };

                let output = HistoryOutput {
                    currency: target_currency,
                    start_date: start,
                    end_date: end,
                    granularity,
                    points: history_points,
                    summary,
                };

                println!("{}", serde_json::to_string_pretty(&output)?);
            }
        },

        None => {
            Cli::command().print_help()?;
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
    let snapshots = storage.get_latest_balances().await?;
    Ok(snapshots
        .into_iter()
        .flat_map(|(account_id, snapshot)| {
            let ts = snapshot.timestamp;
            snapshot.balances.into_iter().map(move |ab| BalanceOutput {
                account_id: account_id.to_string(),
                asset: serde_json::to_value(&ab.asset).unwrap_or_default(),
                amount: ab.amount,
                timestamp: ts.to_rfc3339(),
            })
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
    let account_ids: Vec<String> = conn
        .state
        .account_ids
        .iter()
        .map(|a| a.to_string())
        .collect();

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

    // Create balance snapshot with single asset
    let asset_balance = AssetBalance::new(asset.clone(), amount);
    let snapshot = BalanceSnapshot::now(vec![asset_balance]);

    // Append balance snapshot
    storage.append_balance_snapshot(&id, &snapshot).await?;

    Ok(serde_json::json!({
        "success": true,
        "balance": {
            "account_id": account_id,
            "asset": serde_json::to_value(&asset)?,
            "amount": amount,
            "timestamp": snapshot.timestamp.to_rfc3339()
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
async fn find_connection(
    storage: &JsonFileStorage,
    id_or_name: &str,
) -> Result<Option<Connection>> {
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
async fn sync_connection(
    storage: &JsonFileStorage,
    id_or_name: &str,
    config: &ResolvedConfig,
) -> Result<serde_json::Value> {
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
        let result = synchronizer
            .sync_with_storage(&mut connection, storage)
            .await?;
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
        let result = synchronizer
            .sync_with_storage(&mut connection, storage)
            .await?;
        result.save(storage).await?;

        // Coinbase doesn't provide prices, so fetch them from configured sources
        let prices_fetched = fetch_crypto_prices(&result, config).await.unwrap_or(0);

        return Ok(serde_json::json!({
            "success": true,
            "connection": {
                "id": conn_id,
                "name": conn_name
            },
            "accounts_synced": result.accounts.len(),
            "prices_stored": prices_fetched,
            "last_sync": result.connection.state.last_sync.as_ref().map(|ls| ls.at.to_rfc3339())
        }));
    }

    Err(anyhow::anyhow!(
        "Unknown synchronizer type: {synchronizer_type}"
    ))
}

/// Store prices from a sync result into the market data store
async fn store_sync_prices(
    result: &keepbook::sync::SyncResult,
    config: &ResolvedConfig,
) -> Result<usize> {
    let market_data_store = JsonlMarketDataStore::new(&config.data_dir);
    let mut count = 0;

    for (_, synced_balances) in &result.balances {
        for sb in synced_balances {
            if let Some(price) = &sb.price {
                market_data_store
                    .put_prices(std::slice::from_ref(price))
                    .await?;
                count += 1;
            }
        }
    }

    Ok(count)
}

/// Fetch prices for crypto assets from configured price sources
async fn fetch_crypto_prices(
    result: &keepbook::sync::SyncResult,
    config: &ResolvedConfig,
) -> Result<usize> {
    use keepbook::market_data::CryptoPriceRouter;
    use std::collections::HashSet;

    // Load crypto price sources from registry
    let mut registry = PriceSourceRegistry::new(&config.data_dir);
    registry.load()?;
    let crypto_sources = registry.build_crypto_sources().await?;

    if crypto_sources.is_empty() {
        tracing::debug!("No crypto price sources configured, skipping price fetch");
        return Ok(0);
    }

    let crypto_router = Arc::new(CryptoPriceRouter::new(crypto_sources));
    let store = Arc::new(JsonlMarketDataStore::new(&config.data_dir));
    let market_data = keepbook::market_data::MarketDataService::new(store, None)
        .with_crypto_router(crypto_router);

    // Collect unique crypto assets from sync result
    let assets: HashSet<Asset> = result
        .balances
        .iter()
        .flat_map(|(_, sbs)| sbs.iter().map(|sb| sb.asset_balance.asset.clone()))
        .filter(|a| matches!(a, Asset::Crypto { .. }))
        .collect();

    let date = chrono::Utc::now().date_naive();
    let mut count = 0;

    for asset in assets {
        match market_data.price_close(&asset, date).await {
            Ok(_) => count += 1,
            Err(e) => tracing::warn!(asset = ?asset, error = %e, "Failed to fetch price"),
        }
    }

    Ok(count)
}

struct AssetPriceCache {
    asset: Asset,
    asset_id: keepbook::market_data::AssetId,
    prices: HashMap<NaiveDate, keepbook::market_data::PricePoint>,
}

struct PriceHistoryRequest<'a> {
    storage: &'a JsonFileStorage,
    config: &'a ResolvedConfig,
    account: Option<&'a str>,
    connection: Option<&'a str>,
    start: Option<&'a str>,
    end: Option<&'a str>,
    interval: &'a str,
    currency: Option<String>,
    include_fx: bool,
}

#[derive(Debug, Clone, Copy)]
enum PriceHistoryInterval {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl PriceHistoryInterval {
    fn parse(value: &str) -> Result<Self> {
        match value.to_lowercase().as_str() {
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            "monthly" => Ok(Self::Monthly),
            "yearly" => Ok(Self::Yearly),
            _ => anyhow::bail!("Invalid interval: {value}. Use: daily, weekly, monthly, yearly"),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }
}

async fn fetch_historical_prices(request: PriceHistoryRequest<'_>) -> Result<PriceHistoryOutput> {
    use keepbook::market_data::{CryptoPriceRouter, EquityPriceRouter, FxRateRouter};

    let PriceHistoryRequest {
        storage,
        config,
        account,
        connection,
        start,
        end,
        interval,
        currency,
        include_fx,
    } = request;

    let (scope, accounts) = resolve_price_history_scope(storage, account, connection).await?;

    let mut assets: HashSet<Asset> = HashSet::new();
    let mut earliest_balance_date: Option<NaiveDate> = None;

    for account in &accounts {
        let snapshots = storage.get_balance_snapshots(&account.id).await?;
        for snapshot in snapshots {
            let date = snapshot.timestamp.date_naive();
            earliest_balance_date = Some(match earliest_balance_date {
                Some(current) => current.min(date),
                None => date,
            });
            for balance in snapshot.balances {
                assets.insert(balance.asset);
            }
        }
    }

    if assets.is_empty() {
        anyhow::bail!("No balances found for selected scope");
    }

    let start_date = match start {
        Some(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .with_context(|| format!("Invalid start date: {value}"))?,
        None => earliest_balance_date.context("No balances found to infer start date")?,
    };

    let end_date = match end {
        Some(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .with_context(|| format!("Invalid end date: {value}"))?,
        None => Utc::now().date_naive(),
    };

    if start_date > end_date {
        anyhow::bail!("Start date must be on or before end date");
    }

    let interval = PriceHistoryInterval::parse(interval)?;
    let anchor_day = start_date.day();
    let anchor_month = start_date.month();
    let aligned_start = align_start_date(start_date, interval, anchor_month, anchor_day);

    let target_currency = currency.unwrap_or_else(|| config.reporting_currency.clone());
    let target_currency_upper = target_currency.to_uppercase();

    let store: Arc<dyn MarketDataStore> = Arc::new(JsonlMarketDataStore::new(&config.data_dir));

    // Load configured price sources
    let mut registry = PriceSourceRegistry::new(&config.data_dir);
    registry.load()?;
    let equity_sources = registry.build_equity_sources().await?;
    let crypto_sources = registry.build_crypto_sources().await?;
    let fx_sources = registry.build_fx_sources().await?;

    let mut market_data = MarketDataService::new(store.clone(), None);
    if !equity_sources.is_empty() {
        market_data =
            market_data.with_equity_router(Arc::new(EquityPriceRouter::new(equity_sources)));
    }
    if !crypto_sources.is_empty() {
        market_data =
            market_data.with_crypto_router(Arc::new(CryptoPriceRouter::new(crypto_sources)));
    }
    if !fx_sources.is_empty() {
        market_data = market_data.with_fx_router(Arc::new(FxRateRouter::new(fx_sources)));
    }

    let lookback_days = 7u32;

    let mut asset_caches = Vec::new();
    for asset in assets {
        let asset_id = keepbook::market_data::AssetId::from_asset(&asset);
        let prices = load_price_cache(&store, &asset_id).await?;
        asset_caches.push(AssetPriceCache {
            asset,
            asset_id,
            prices,
        });
    }

    asset_caches.sort_by(|a, b| a.asset_id.to_string().cmp(&b.asset_id.to_string()));

    let mut fx_cache: HashMap<(String, String), HashMap<NaiveDate, FxRatePoint>> = HashMap::new();

    if include_fx {
        for asset_cache in &asset_caches {
            if let Asset::Currency { iso_code } = &asset_cache.asset {
                let base = iso_code.to_uppercase();
                if base == target_currency_upper {
                    continue;
                }
                let key = (base.clone(), target_currency_upper.clone());
                if !fx_cache.contains_key(&key) {
                    fx_cache.insert(key.clone(), load_fx_cache(&store, &key.0, &key.1).await?);
                }
            }
        }
    }

    let mut price_stats = PriceHistoryStats::default();
    let mut fx_stats = PriceHistoryStats::default();
    let mut failures = Vec::new();
    let mut failure_count = 0usize;
    let failure_limit = 50usize;

    let mut current = aligned_start;
    let mut points = 0usize;
    {
        let mut fx_ctx = FxRateContext {
            market_data: &market_data,
            store: &store,
            fx_cache: &mut fx_cache,
            stats: &mut fx_stats,
            failures: &mut failures,
            failure_count: &mut failure_count,
            failure_limit,
            lookback_days,
        };

        while current <= end_date {
            points += 1;
            for asset_cache in asset_caches.iter_mut() {
                match &asset_cache.asset {
                    Asset::Currency { iso_code } => {
                        if include_fx {
                            let base = iso_code.to_uppercase();
                            if base != target_currency_upper {
                                ensure_fx_rate(&mut fx_ctx, &base, &target_currency_upper, current)
                                    .await?;
                            }
                        }
                    }
                    Asset::Equity { .. } | Asset::Crypto { .. } => {
                        price_stats.attempted += 1;
                        if let Some((price, exact)) =
                            resolve_cached_price(&asset_cache.prices, current, lookback_days)
                        {
                            if exact {
                                price_stats.existing += 1;
                            } else {
                                price_stats.lookback += 1;
                            }

                            if include_fx
                                && price.quote_currency.to_uppercase() != target_currency_upper
                            {
                                ensure_fx_rate(
                                    &mut fx_ctx,
                                    &price.quote_currency.to_uppercase(),
                                    &target_currency_upper,
                                    current,
                                )
                                .await?;
                            }
                            continue;
                        }

                        match market_data.price_close(&asset_cache.asset, current).await {
                            Ok(price) => {
                                let exact = price.as_of_date == current;
                                if exact {
                                    price_stats.fetched += 1;
                                } else {
                                    price_stats.lookback += 1;
                                }

                                upsert_price_cache(&mut asset_cache.prices, price.clone());

                                if include_fx
                                    && price.quote_currency.to_uppercase() != target_currency_upper
                                {
                                    ensure_fx_rate(
                                        &mut fx_ctx,
                                        &price.quote_currency.to_uppercase(),
                                        &target_currency_upper,
                                        current,
                                    )
                                    .await?;
                                }
                            }
                            Err(e) => {
                                price_stats.missing += 1;
                                *fx_ctx.failure_count += 1;
                                if fx_ctx.failures.len() < fx_ctx.failure_limit {
                                    fx_ctx.failures.push(PriceHistoryFailure {
                                        kind: "price".to_string(),
                                        date: current.to_string(),
                                        error: e.to_string(),
                                        asset_id: Some(asset_cache.asset_id.to_string()),
                                        asset: Some(asset_cache.asset.clone()),
                                        base: None,
                                        quote: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            current = advance_interval_date(current, interval, anchor_day, anchor_month);
        }
    }

    let days = (end_date - start_date).num_days() as usize + 1;

    let assets_output = asset_caches
        .iter()
        .map(|cache| AssetInfoOutput {
            asset: cache.asset.clone(),
            asset_id: cache.asset_id.to_string(),
        })
        .collect();

    Ok(PriceHistoryOutput {
        scope,
        currency: target_currency,
        interval: interval.as_str().to_string(),
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        earliest_balance_date: earliest_balance_date.map(|d| d.to_string()),
        days,
        points,
        assets: assets_output,
        prices: price_stats,
        fx: if include_fx { Some(fx_stats) } else { None },
        failure_count,
        failures,
    })
}

fn advance_interval_date(
    date: NaiveDate,
    interval: PriceHistoryInterval,
    anchor_day: u32,
    anchor_month: u32,
) -> NaiveDate {
    match interval {
        PriceHistoryInterval::Daily => date + Duration::days(1),
        PriceHistoryInterval::Weekly => date + Duration::days(7),
        PriceHistoryInterval::Monthly => next_month_end(date),
        PriceHistoryInterval::Yearly => add_years(date, 1, anchor_month, anchor_day),
    }
}

fn align_start_date(
    date: NaiveDate,
    interval: PriceHistoryInterval,
    anchor_month: u32,
    anchor_day: u32,
) -> NaiveDate {
    match interval {
        PriceHistoryInterval::Monthly => month_end(date),
        PriceHistoryInterval::Yearly => {
            let day = anchor_day.min(days_in_month(date.year(), anchor_month));
            NaiveDate::from_ymd_opt(date.year(), anchor_month, day).expect("valid yearly date")
        }
        _ => date,
    }
}

fn add_years(date: NaiveDate, years: i32, anchor_month: u32, anchor_day: u32) -> NaiveDate {
    let year = date.year() + years;
    let day = anchor_day.min(days_in_month(year, anchor_month));
    NaiveDate::from_ymd_opt(year, anchor_month, day).expect("valid yearly date")
}

fn next_month_end(date: NaiveDate) -> NaiveDate {
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    let day = days_in_month(year, month);
    NaiveDate::from_ymd_opt(year, month, day).expect("valid next month end")
}

fn month_end(date: NaiveDate) -> NaiveDate {
    let day = days_in_month(date.year(), date.month());
    NaiveDate::from_ymd_opt(date.year(), date.month(), day).expect("valid month end")
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next =
        NaiveDate::from_ymd_opt(next_year, next_month, 1).expect("valid next month");
    let last = first_next - Duration::days(1);
    last.day()
}

async fn resolve_price_history_scope(
    storage: &JsonFileStorage,
    account: Option<&str>,
    connection: Option<&str>,
) -> Result<(PriceHistoryScopeOutput, Vec<Account>)> {
    if account.is_some() && connection.is_some() {
        anyhow::bail!("Specify only one of --account or --connection");
    }

    if let Some(id_or_name) = account {
        let account = find_account(storage, id_or_name)
            .await?
            .context(format!("Account not found: {id_or_name}"))?;
        return Ok((
            PriceHistoryScopeOutput::Account {
                id: account.id.to_string(),
                name: account.name.clone(),
            },
            vec![account],
        ));
    }

    if let Some(id_or_name) = connection {
        let connection = find_connection(storage, id_or_name)
            .await?
            .context(format!("Connection not found: {id_or_name}"))?;
        let mut accounts = Vec::new();

        if !connection.state.account_ids.is_empty() {
            for account_id in &connection.state.account_ids {
                match storage.get_account(account_id).await? {
                    Some(account) => accounts.push(account),
                    None => {
                        tracing::warn!(
                            connection_id = %connection.id(),
                            account_id = %account_id,
                            "account referenced by connection not found"
                        );
                    }
                }
            }
        } else {
            accounts = storage
                .list_accounts()
                .await?
                .into_iter()
                .filter(|a| a.connection_id == *connection.id())
                .collect();
        }

        if accounts.is_empty() {
            anyhow::bail!("No accounts found for connection {}", connection.name());
        }

        return Ok((
            PriceHistoryScopeOutput::Connection {
                id: connection.id().to_string(),
                name: connection.name().to_string(),
            },
            accounts,
        ));
    }

    let accounts = storage.list_accounts().await?;
    if accounts.is_empty() {
        anyhow::bail!("No accounts found");
    }

    Ok((PriceHistoryScopeOutput::Portfolio, accounts))
}

async fn find_account(storage: &JsonFileStorage, id_or_name: &str) -> Result<Option<Account>> {
    let id = Id::from_string(id_or_name);
    if let Some(account) = storage.get_account(&id).await? {
        return Ok(Some(account));
    }

    let accounts = storage.list_accounts().await?;
    let mut matches: Vec<Account> = accounts
        .into_iter()
        .filter(|a| a.name.eq_ignore_ascii_case(id_or_name))
        .collect();

    if matches.is_empty() {
        return Ok(None);
    }

    if matches.len() > 1 {
        let ids: Vec<String> = matches.iter().map(|a| a.id.to_string()).collect();
        anyhow::bail!("Multiple accounts named '{id_or_name}'. Use an ID instead: {ids:?}");
    }

    Ok(matches.pop())
}

async fn load_price_cache(
    store: &Arc<dyn MarketDataStore>,
    asset_id: &keepbook::market_data::AssetId,
) -> Result<HashMap<NaiveDate, PricePoint>> {
    let prices = store.get_all_prices(asset_id).await?;
    let mut map: HashMap<NaiveDate, PricePoint> = HashMap::new();

    for price in prices {
        if price.kind != PriceKind::Close {
            continue;
        }
        match map.get(&price.as_of_date) {
            Some(existing) if existing.timestamp >= price.timestamp => {}
            _ => {
                map.insert(price.as_of_date, price);
            }
        }
    }

    Ok(map)
}

async fn load_fx_cache(
    store: &Arc<dyn MarketDataStore>,
    base: &str,
    quote: &str,
) -> Result<HashMap<NaiveDate, FxRatePoint>> {
    let rates = store.get_all_fx_rates(base, quote).await?;
    let mut map: HashMap<NaiveDate, FxRatePoint> = HashMap::new();

    for rate in rates {
        if rate.kind != FxRateKind::Close {
            continue;
        }
        match map.get(&rate.as_of_date) {
            Some(existing) if existing.timestamp >= rate.timestamp => {}
            _ => {
                map.insert(rate.as_of_date, rate);
            }
        }
    }

    Ok(map)
}

fn resolve_cached_price(
    cache: &HashMap<NaiveDate, PricePoint>,
    date: NaiveDate,
    lookback_days: u32,
) -> Option<(PricePoint, bool)> {
    if let Some(price) = cache.get(&date) {
        return Some((price.clone(), true));
    }

    for offset in 1..=lookback_days {
        let target = date - Duration::days(offset as i64);
        if let Some(price) = cache.get(&target) {
            return Some((price.clone(), false));
        }
    }

    None
}

fn resolve_cached_fx(
    cache: &HashMap<NaiveDate, FxRatePoint>,
    date: NaiveDate,
    lookback_days: u32,
) -> Option<(FxRatePoint, bool)> {
    if let Some(rate) = cache.get(&date) {
        return Some((rate.clone(), true));
    }

    for offset in 1..=lookback_days {
        let target = date - Duration::days(offset as i64);
        if let Some(rate) = cache.get(&target) {
            return Some((rate.clone(), false));
        }
    }

    None
}

fn upsert_price_cache(cache: &mut HashMap<NaiveDate, PricePoint>, price: PricePoint) -> bool {
    match cache.get(&price.as_of_date) {
        Some(existing) if existing.timestamp >= price.timestamp => false,
        _ => {
            cache.insert(price.as_of_date, price);
            true
        }
    }
}

fn upsert_fx_cache(cache: &mut HashMap<NaiveDate, FxRatePoint>, rate: FxRatePoint) -> bool {
    match cache.get(&rate.as_of_date) {
        Some(existing) if existing.timestamp >= rate.timestamp => false,
        _ => {
            cache.insert(rate.as_of_date, rate);
            true
        }
    }
}

struct FxRateContext<'a> {
    market_data: &'a MarketDataService,
    store: &'a Arc<dyn MarketDataStore>,
    fx_cache: &'a mut HashMap<(String, String), HashMap<NaiveDate, FxRatePoint>>,
    stats: &'a mut PriceHistoryStats,
    failures: &'a mut Vec<PriceHistoryFailure>,
    failure_count: &'a mut usize,
    failure_limit: usize,
    lookback_days: u32,
}

async fn ensure_fx_rate(
    ctx: &mut FxRateContext<'_>,
    base: &str,
    quote: &str,
    date: NaiveDate,
) -> Result<()> {
    ctx.stats.attempted += 1;

    let base_upper = base.to_uppercase();
    let quote_upper = quote.to_uppercase();
    let key = (base_upper.clone(), quote_upper.clone());

    if !ctx.fx_cache.contains_key(&key) {
        ctx.fx_cache.insert(
            key.clone(),
            load_fx_cache(ctx.store, &base_upper, &quote_upper).await?,
        );
    }

    let cache = ctx
        .fx_cache
        .get(&key)
        .expect("fx cache should be initialized");

    if let Some((_, exact)) = resolve_cached_fx(cache, date, ctx.lookback_days) {
        if exact {
            ctx.stats.existing += 1;
        } else {
            ctx.stats.lookback += 1;
        }
        return Ok(());
    }

    match ctx
        .market_data
        .fx_close(&base_upper, &quote_upper, date)
        .await
    {
        Ok(rate) => {
            if rate.as_of_date == date {
                ctx.stats.fetched += 1;
            } else {
                ctx.stats.lookback += 1;
            }
            if let Some(cache) = ctx.fx_cache.get_mut(&key) {
                upsert_fx_cache(cache, rate);
            }
        }
        Err(e) => {
            ctx.stats.missing += 1;
            *ctx.failure_count += 1;
            if ctx.failures.len() < ctx.failure_limit {
                ctx.failures.push(PriceHistoryFailure {
                    kind: "fx".to_string(),
                    date: date.to_string(),
                    error: e.to_string(),
                    asset_id: None,
                    asset: None,
                    base: Some(base_upper),
                    quote: Some(quote_upper),
                });
            }
        }
    }

    Ok(())
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
async fn schwab_login(
    storage: &JsonFileStorage,
    id_or_name: Option<&str>,
) -> Result<serde_json::Value> {
    // Find Schwab connection(s)
    let connections = storage.list_connections().await?;
    let schwab_connections: Vec<_> = connections
        .into_iter()
        .filter(|c| c.config.synchronizer == "schwab")
        .collect();

    let connection = match (id_or_name, schwab_connections.len()) {
        // Explicit ID/name provided
        (Some(id_or_name), _) => find_connection(storage, id_or_name)
            .await?
            .filter(|c| c.config.synchronizer == "schwab")
            .context(format!("Schwab connection not found: {id_or_name}"))?,
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
