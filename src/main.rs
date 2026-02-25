use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use keepbook::app;
use keepbook::config::{default_config_path, ResolvedConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::TransactionSyncMode;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

const CLI_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (git commit ",
    env!("GIT_COMMIT_HASH"),
    ")"
);

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
#[command(version = CLI_VERSION)]
#[command(about = "Personal finance manager")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value_os_t = default_config_path())]
    config: PathBuf,

    /// Merge origin/master before executing the command.
    #[arg(long, global = true, conflicts_with = "skip_git_merge_master")]
    git_merge_master: bool,

    /// Skip merging origin/master even if enabled in config.
    #[arg(
        long = "skip-git-merge-master",
        global = true,
        conflicts_with = "git_merge_master"
    )]
    skip_git_merge_master: bool,

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

    /// Import data from exported files
    #[command(subcommand)]
    Import(ImportCommand),

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

    /// Spending reports based on transaction logs
    Spending {
        /// Period granularity: daily, weekly, monthly, quarterly, yearly, range, custom
        #[arg(long, default_value = "monthly")]
        period: String,

        /// Start date (YYYY-MM-DD, default: earliest matching transaction)
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD, default: today in the selected timezone)
        #[arg(long)]
        end: Option<String>,

        /// Reporting currency (default: from config)
        #[arg(long)]
        currency: Option<String>,

        /// Timezone for bucketing and date filtering (IANA name, default: local)
        #[arg(long)]
        tz: Option<String>,

        /// Week start day for weekly periods: sunday or monday (default: sunday)
        #[arg(long)]
        week_start: Option<String>,

        /// Custom bucket size (period=custom only). Must be a positive multiple of 1d (e.g. "14d").
        #[arg(long, value_name = "DURATION", value_parser = parse_duration_arg)]
        bucket: Option<std::time::Duration>,

        /// Filter to a single account by ID or name (mutually exclusive with --connection)
        #[arg(long)]
        account: Option<String>,

        /// Filter to a single connection by ID or name (mutually exclusive with --account)
        #[arg(long)]
        connection: Option<String>,

        /// Transaction status filter: posted, posted+pending, all (default: posted)
        #[arg(long, default_value = "posted")]
        status: String,

        /// Direction: outflow, inflow, net (default: outflow)
        #[arg(long, default_value = "outflow")]
        direction: String,

        /// Grouping: none, category, merchant, account, tag (default: none)
        #[arg(long, default_value = "none")]
        group_by: String,

        /// Limit breakdown rows per period (when grouping)
        #[arg(long)]
        top: Option<usize>,

        /// Look back this many days for cached close prices / FX rates (default: 7)
        #[arg(long, default_value_t = 7)]
        lookback_days: u32,

        /// Include non-currency assets (equity/crypto) by valuing them using cached close prices.
        ///
        /// Default is currency-only spending (still supports FX conversion for currency txns).
        #[arg(long, default_value_t = false)]
        include_noncurrency: bool,

        /// Emit empty periods with total 0 and transaction_count 0.
        ///
        /// Default output is sparse (only periods with non-zero totals).
        #[arg(long, default_value_t = false)]
        include_empty: bool,
    },

    /// Spending report grouped by category
    SpendingCategories {
        /// Period granularity: daily, weekly, monthly, quarterly, yearly, range, custom
        #[arg(long, default_value = "monthly")]
        period: String,

        /// Start date (YYYY-MM-DD, default: earliest matching transaction)
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD, default: today in the selected timezone)
        #[arg(long)]
        end: Option<String>,

        /// Reporting currency (default: from config)
        #[arg(long)]
        currency: Option<String>,

        /// Timezone for bucketing and date filtering (IANA name, default: local)
        #[arg(long)]
        tz: Option<String>,

        /// Week start day for weekly periods: sunday or monday (default: sunday)
        #[arg(long)]
        week_start: Option<String>,

        /// Custom bucket size (period=custom only). Must be a positive multiple of 1d (e.g. "14d").
        #[arg(long, value_name = "DURATION", value_parser = parse_duration_arg)]
        bucket: Option<std::time::Duration>,

        /// Filter to a single account by ID or name (mutually exclusive with --connection)
        #[arg(long)]
        account: Option<String>,

        /// Filter to a single connection by ID or name (mutually exclusive with --account)
        #[arg(long)]
        connection: Option<String>,

        /// Transaction status filter: posted, posted+pending, all (default: posted)
        #[arg(long, default_value = "posted")]
        status: String,

        /// Direction: outflow, inflow, net (default: outflow)
        #[arg(long, default_value = "outflow")]
        direction: String,

        /// Limit category rows per period
        #[arg(long)]
        top: Option<usize>,

        /// Look back this many days for cached close prices / FX rates (default: 7)
        #[arg(long, default_value_t = 7)]
        lookback_days: u32,

        /// Include non-currency assets (equity/crypto) by valuing them using cached close prices.
        ///
        /// Default is currency-only spending (still supports FX conversion for currency txns).
        #[arg(long, default_value_t = false)]
        include_noncurrency: bool,

        /// Emit empty periods with total 0 and transaction_count 0.
        ///
        /// Default output is sparse (only periods with non-zero totals).
        #[arg(long, default_value_t = false)]
        include_empty: bool,
    },
}

#[derive(Subcommand)]
enum AddCommand {
    /// Add a new connection
    Connection {
        /// Name for the connection
        name: String,

        /// Synchronizer to use (default: manual)
        #[arg(long, default_value = "manual")]
        synchronizer: String,
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

    /// Set account-level configuration values
    AccountConfig {
        /// Account ID or name
        #[arg(long)]
        account: String,

        /// Balance backfill policy: none, zero, carry_earliest
        #[arg(long, conflicts_with = "clear_balance_backfill")]
        balance_backfill: Option<String>,

        /// Clear balance backfill policy override
        #[arg(long)]
        clear_balance_backfill: bool,
    },

    /// Set a transaction annotation (append-only patch)
    Transaction {
        /// Account ID
        #[arg(long)]
        account: String,

        /// Transaction ID
        #[arg(long)]
        transaction: String,

        /// Override description
        #[arg(long, conflicts_with = "clear_description")]
        description: Option<String>,

        /// Clear description override
        #[arg(long)]
        clear_description: bool,

        /// Set note
        #[arg(long, conflicts_with = "clear_note")]
        note: Option<String>,

        /// Clear note
        #[arg(long)]
        clear_note: bool,

        /// Set category
        #[arg(long, conflicts_with = "clear_category")]
        category: Option<String>,

        /// Clear category
        #[arg(long)]
        clear_category: bool,

        /// Set tags (repeatable)
        #[arg(long, short)]
        tag: Vec<String>,

        /// Set tags to empty array
        #[arg(long, conflicts_with = "clear_tags")]
        tags_empty: bool,

        /// Clear tags field
        #[arg(long)]
        clear_tags: bool,
    },
}

#[derive(Subcommand)]
enum ImportCommand {
    /// Schwab import commands
    #[command(subcommand)]
    Schwab(SchwabImportCommand),
}

#[derive(Subcommand)]
enum SchwabImportCommand {
    /// Import transactions from a Schwab JSON export file
    Transactions {
        /// Account ID or name
        #[arg(long)]
        account: String,

        /// Path to Schwab-exported JSON file
        file: PathBuf,
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
        /// Transaction sync mode (auto: stop when overlap detected; full: backfill as far as possible)
        #[arg(long, value_enum, default_value = "auto")]
        transactions: TransactionsModeArg,
    },
    /// Sync all connections
    All {
        /// Only sync connections with stale data
        #[arg(long)]
        if_stale: bool,
        /// Transaction sync mode (auto: stop when overlap detected; full: backfill as far as possible)
        #[arg(long, value_enum, default_value = "auto")]
        transactions: TransactionsModeArg,
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
    /// Recompact account JSONL files (dedupe append-only logs and sort chronologically)
    Recompact,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TransactionsModeArg {
    Auto,
    Full,
}

impl From<TransactionsModeArg> for TransactionSyncMode {
    fn from(value: TransactionsModeArg) -> Self {
        match value {
            TransactionsModeArg::Auto => TransactionSyncMode::Auto,
            TransactionsModeArg::Full => TransactionSyncMode::Full,
        }
    }
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
    Transactions {
        /// Start date (YYYY-MM-DD, default: 30 days ago)
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD, default: today)
        #[arg(long)]
        end: Option<String>,

        /// Sort transactions by amount (ascending)
        #[arg(long, default_value_t = false)]
        sort_by_amount: bool,

        /// Include transactions that would otherwise be ignored by spending/list ignore rules
        #[arg(long, default_value_t = false)]
        include_ignored: bool,
    },

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

    // Pre-command hook (decoupled from CLI parsing; CLI only computes enablement).
    let merge_enabled = if cli.git_merge_master {
        true
    } else if cli.skip_git_merge_master {
        false
    } else {
        config.git.merge_master_before_command
    };
    app::run_preflight(
        &config,
        app::PreflightOptions {
            merge_origin_master: merge_enabled,
        },
    )?;

    match cli.command {
        Some(Command::Config) => {
            let output = app::config_output(&cli.config, &config);
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Some(Command::Add(add_cmd)) => match add_cmd {
            AddCommand::Connection { name, synchronizer } => {
                let result =
                    app::add_connection(storage_arc.as_ref(), &config, &name, &synchronizer)
                        .await?;
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
            SetCommand::AccountConfig {
                account,
                balance_backfill,
                clear_balance_backfill,
            } => {
                let result = app::set_account_config(
                    storage_arc.as_ref(),
                    &config,
                    &account,
                    balance_backfill.as_deref(),
                    clear_balance_backfill,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SetCommand::Transaction {
                account,
                transaction,
                description,
                clear_description,
                note,
                clear_note,
                category,
                clear_category,
                tag,
                tags_empty,
                clear_tags,
            } => {
                let result = app::set_transaction_annotation(
                    storage_arc.as_ref(),
                    &config,
                    &account,
                    &transaction,
                    description,
                    clear_description,
                    note,
                    clear_note,
                    category,
                    clear_category,
                    tag,
                    tags_empty,
                    clear_tags,
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },

        Some(Command::Import(import_cmd)) => match import_cmd {
            ImportCommand::Schwab(schwab_cmd) => match schwab_cmd {
                SchwabImportCommand::Transactions { account, file } => {
                    let result = app::import_schwab_transactions(
                        storage_arc.as_ref(),
                        &config,
                        &account,
                        &file,
                    )
                    .await?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
            },
        },

        Some(Command::Sync(sync_cmd)) => match sync_cmd {
            SyncCommand::Connection {
                id_or_name,
                if_stale,
                transactions,
            } => {
                let transactions: TransactionSyncMode = transactions.into();
                let result = if if_stale {
                    app::sync_connection_if_stale(
                        storage_arc.clone(),
                        &config,
                        &id_or_name,
                        transactions,
                    )
                    .await?
                } else {
                    app::sync_connection(storage_arc.clone(), &config, &id_or_name, transactions)
                        .await?
                };
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            SyncCommand::All {
                if_stale,
                transactions,
            } => {
                let transactions: TransactionSyncMode = transactions.into();
                let result = if if_stale {
                    app::sync_all_if_stale(storage_arc.clone(), &config, transactions).await?
                } else {
                    app::sync_all(storage_arc.clone(), &config, transactions).await?
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
            SyncCommand::Recompact => {
                let result = app::sync_recompact(&storage, &config).await?;
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
                let balances = app::list_balances(storage_arc.as_ref(), &config).await?;
                println!("{}", serde_json::to_string_pretty(&balances)?);
            }

            ListCommand::Transactions {
                start,
                end,
                sort_by_amount,
                include_ignored,
            } => {
                let transactions =
                    app::list_transactions(
                        storage_arc.as_ref(),
                        start,
                        end,
                        sort_by_amount,
                        !include_ignored,
                        &config,
                    )
                    .await?;
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

        Some(Command::Spending {
            period,
            start,
            end,
            currency,
            tz,
            week_start,
            bucket,
            account,
            connection,
            status,
            direction,
            group_by,
            top,
            lookback_days,
            include_noncurrency,
            include_empty,
        }) => {
            let output = app::spending_report(
                storage_arc.as_ref(),
                &config,
                app::SpendingReportOptions {
                    currency,
                    start,
                    end,
                    period,
                    tz,
                    week_start,
                    bucket,
                    account,
                    connection,
                    status,
                    direction,
                    group_by,
                    top,
                    lookback_days,
                    include_noncurrency,
                    include_empty,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        Some(Command::SpendingCategories {
            period,
            start,
            end,
            currency,
            tz,
            week_start,
            bucket,
            account,
            connection,
            status,
            direction,
            top,
            lookback_days,
            include_noncurrency,
            include_empty,
        }) => {
            let output = app::spending_report(
                storage_arc.as_ref(),
                &config,
                app::SpendingReportOptions {
                    currency,
                    start,
                    end,
                    period,
                    tz,
                    week_start,
                    bucket,
                    account,
                    connection,
                    status,
                    direction,
                    group_by: "category".to_string(),
                    top,
                    lookback_days,
                    include_noncurrency,
                    include_empty,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        None => {
            Cli::command().print_help()?;
        }
    }

    Ok(())
}
