use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand};
use keepbook::app;
use keepbook::config::{default_config_path, ResolvedConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn parse_duration_arg(s: &str) -> Result<std::time::Duration, String> {
    keepbook::duration::parse_duration(s).map_err(|e| e.to_string())
}

#[derive(Args, Debug, Clone)]
struct PriceSyncOptions {
    /// Force fetching even if cached data looks fresh (best-effort for quotes).
    #[arg(long, global = true)]
    force: bool,

    /// Override quote freshness threshold (e.g. "0s", "30m", "6h", "1d").
    /// Default is `refresh.price_staleness` from config.
    #[arg(
        long,
        global = true,
        value_name = "DURATION",
        value_parser = parse_duration_arg
    )]
    quote_staleness: Option<std::time::Duration>,
}

#[derive(Subcommand, Debug, Clone)]
enum PriceSyncScopeCommand {
    /// Refresh prices for all accounts (uses latest stored balances).
    All,
    /// Refresh prices for accounts in a connection.
    /// If ID/NAME is omitted, you will be prompted to select one.
    Connection {
        /// Connection ID or name
        id_or_name: Option<String>,
    },
    /// Refresh prices for a single account.
    /// If ID/NAME is omitted, you will be prompted to select one.
    Account {
        /// Account ID or name
        id_or_name: Option<String>,
    },
}

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
    /// Refresh prices only (no balance sync).
    ///
    /// If no scope subcommand is specified, a minimal interactive selector is shown.
    Prices {
        #[command(flatten)]
        opts: PriceSyncOptions,

        #[command(subcommand)]
        scope: Option<PriceSyncScopeCommand>,
    },
    /// Rebuild all symlinks (connections/by-name and account directories)
    Symlinks,
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Schwab authentication commands
    #[command(subcommand)]
    Schwab(SchwabAuthCommand),
    /// Chase authentication commands
    #[command(subcommand)]
    Chase(ChaseAuthCommand),
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
enum ChaseAuthCommand {
    /// Login via browser to capture session
    Login {
        /// Connection ID or name (optional if only one Chase connection)
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

        /// Interval for backfill: daily, weekly, monthly, yearly/annual (default: monthly)
        #[arg(long, default_value = "monthly")]
        interval: String,

        /// Look back this many days when a close price is missing (default: 7)
        #[arg(long, default_value_t = 7)]
        lookback_days: u32,

        /// Delay (ms) between price fetches to avoid rate limits (default: 0)
        #[arg(long, default_value_t = 0)]
        request_delay_ms: u64,

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

        /// Include price changes as change points (default: enabled)
        #[arg(long, default_value_t = true)]
        include_prices: bool,

        /// Disable price changes as change points (faster, less detailed)
        #[arg(long, conflicts_with = "include_prices")]
        no_include_prices: bool,
    },

    /// List all change points (timestamps where portfolio value could have changed)
    ChangePoints {
        /// Start date (YYYY-MM-DD, default: earliest data)
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD, default: today)
        #[arg(long)]
        end: Option<String>,

        /// Time granularity: none/full, hourly, daily, weekly, monthly, yearly (default: none)
        #[arg(long, default_value = "none")]
        granularity: String,

        /// Include price changes as change points (default: enabled)
        #[arg(long, default_value_t = true)]
        include_prices: bool,

        /// Disable price changes as change points (faster, less detailed)
        #[arg(long, conflicts_with = "include_prices")]
        no_include_prices: bool,
    },
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
    let storage_arc: Arc<dyn Storage> = Arc::new(storage.clone());

    match cli.command {
        Some(Command::Config) => {
            let output = app::config_output(&cli.config, &config);
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Some(Command::Add(add_cmd)) => match add_cmd {
            AddCommand::Connection { name } => {
                let result = app::add_connection(storage_arc.as_ref(), &config, &name).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            AddCommand::Account {
                connection,
                name,
                tag,
            } => {
                let result =
                    app::add_account(storage_arc.as_ref(), &config, &connection, &name, tag)
                        .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Remove(remove_cmd)) => match remove_cmd {
            RemoveCommand::Connection { id } => {
                let result = app::remove_connection(storage_arc.as_ref(), &config, &id).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Set(set_cmd)) => match set_cmd {
            SetCommand::Balance {
                account,
                asset,
                amount,
            } => {
                let result =
                    app::set_balance(storage_arc.as_ref(), &config, &account, &asset, &amount)
                        .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Sync(sync_cmd)) => match sync_cmd {
            SyncCommand::Connection {
                id_or_name,
                if_stale,
            } => {
                let result = if if_stale {
                    app::sync_connection_if_stale(storage_arc.clone(), &config, &id_or_name).await?
                } else {
                    app::sync_connection(storage_arc.clone(), &config, &id_or_name).await?
                };
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SyncCommand::All { if_stale } => {
                let result = if if_stale {
                    app::sync_all_if_stale(storage_arc.clone(), &config).await?
                } else {
                    app::sync_all(storage_arc.clone(), &config).await?
                };
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SyncCommand::Prices { opts, scope } => {
                let result = app::sync_prices(
                    storage_arc.clone(),
                    &config,
                    match &scope {
                        None => app::SyncPricesScopeArg::Interactive,
                        Some(PriceSyncScopeCommand::All) => app::SyncPricesScopeArg::All,
                        Some(PriceSyncScopeCommand::Connection { id_or_name }) => {
                            app::SyncPricesScopeArg::Connection(id_or_name.as_deref())
                        }
                        Some(PriceSyncScopeCommand::Account { id_or_name }) => {
                            app::SyncPricesScopeArg::Account(id_or_name.as_deref())
                        }
                    },
                    opts.force,
                    opts.quote_staleness,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SyncCommand::Symlinks => {
                let result = app::sync_symlinks(&storage, &config).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Auth(auth_cmd)) => match auth_cmd {
            AuthCommand::Schwab(schwab_cmd) => match schwab_cmd {
                SchwabAuthCommand::Login { id_or_name } => {
                    let result =
                        app::schwab_login(storage_arc.clone(), &config, id_or_name.as_deref())
                            .await?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
            },
            AuthCommand::Chase(chase_cmd) => match chase_cmd {
                ChaseAuthCommand::Login { id_or_name } => {
                    let result =
                        app::chase_login(storage_arc.clone(), &config, id_or_name.as_deref())
                            .await?;
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
                lookback_days,
                request_delay_ms,
                currency,
                no_fx,
            } => {
                let output = app::fetch_historical_prices(app::PriceHistoryRequest {
                    storage: storage_arc.as_ref(),
                    config: &config,
                    account: account.as_deref(),
                    connection: connection.as_deref(),
                    start: start.as_deref(),
                    end: end.as_deref(),
                    interval: interval.as_str(),
                    lookback_days,
                    request_delay_ms,
                    currency,
                    include_fx: !no_fx,
                })
                .await?;
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
        },

        Some(Command::List(list_cmd)) => match list_cmd {
            ListCommand::Connections => {
                let connections = app::list_connections(storage_arc.as_ref()).await?;
                println!("{}", serde_json::to_string_pretty(&connections)?);
            }

            ListCommand::Accounts => {
                let accounts = app::list_accounts(storage_arc.as_ref()).await?;
                println!("{}", serde_json::to_string_pretty(&accounts)?);
            }

            ListCommand::PriceSources => {
                let sources = app::list_price_sources(&config.data_dir)?;
                println!("{}", serde_json::to_string_pretty(&sources)?);
            }

            ListCommand::Balances => {
                let balances = app::list_balances(storage_arc.as_ref()).await?;
                println!("{}", serde_json::to_string_pretty(&balances)?);
            }

            ListCommand::Transactions => {
                let transactions = app::list_transactions(storage_arc.as_ref()).await?;
                println!("{}", serde_json::to_string_pretty(&transactions)?);
            }

            ListCommand::All => {
                let output = app::list_all(storage_arc.as_ref(), &config).await?;
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
                let snapshot = app::portfolio_snapshot(
                    storage_arc.clone(),
                    &config,
                    currency,
                    date,
                    group_by,
                    detail,
                    auto,
                    offline,
                    dry_run,
                    force_refresh,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            }

            PortfolioCommand::History {
                currency,
                start,
                end,
                granularity,
                include_prices,
                no_include_prices,
            } => {
                let output = app::portfolio_history(
                    storage_arc.clone(),
                    &config,
                    currency,
                    start,
                    end,
                    granularity,
                    include_prices && !no_include_prices,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&output)?);
            }

            PortfolioCommand::ChangePoints {
                start,
                end,
                granularity,
                include_prices,
                no_include_prices,
            } => {
                let output = app::portfolio_change_points(
                    storage_arc.clone(),
                    &config,
                    start,
                    end,
                    granularity,
                    include_prices && !no_include_prices,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
        },

        None => {
            Cli::command().print_help()?;
        }
    }

    Ok(())
}
