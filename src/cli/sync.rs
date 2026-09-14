use clap::{Args, Subcommand, ValueEnum};
use keepbook::sync::TransactionSyncMode;

use super::parse_duration_arg;

#[derive(Args, Debug, Clone)]
pub struct PriceSyncOptions {
    /// Force fetching even if cached data looks fresh (best-effort for quotes).
    #[arg(long, global = true)]
    pub force: bool,

    /// Override quote freshness threshold (e.g. "0s", "30m", "6h", "1d").
    /// Default is `refresh.price_staleness` from config.
    #[arg(
        long,
        global = true,
        value_name = "DURATION",
        value_parser = parse_duration_arg
    )]
    pub quote_staleness: Option<std::time::Duration>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PriceSyncScopeCommand {
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

#[derive(Subcommand)]
pub enum SyncCommand {
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
    /// Recompact account and market-data JSONL files (dedupe append-only logs and sort canonically)
    Recompact,
    /// Persist backfilled standardized transaction metadata to JSONL without compaction
    BackfillMetadata,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TransactionsModeArg {
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
