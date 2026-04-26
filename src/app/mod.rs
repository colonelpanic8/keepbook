mod config;
mod graph;
mod ignore_rules;
mod import;
mod list;
mod mutations;
mod portfolio;
mod preflight;
mod spending;
mod sync;
mod types;
mod value;

use crate::config::ResolvedConfig;
use crate::sync::{AutoCommitter, GitAutoCommitter};

pub use config::config_output;
pub use graph::{portfolio_graph, PortfolioGraphOptions, PortfolioGraphOutput};
pub use import::import_schwab_transactions;
pub use list::{
    list_accounts, list_all, list_balances, list_connections, list_price_sources, list_transactions,
};
pub use mutations::{
    add_account, add_account_with, add_connection, add_connection_with, parse_asset,
    remove_connection, set_account_config, set_balance, set_transaction_annotation,
};
pub use portfolio::{
    default_portfolio_change_points_granularity, default_portfolio_history_granularity,
    default_portfolio_include_prices, fetch_historical_prices, fill_prices_at_date,
    portfolio_change_points, portfolio_history, portfolio_recent_history, portfolio_snapshot,
    PriceHistoryRequest, DEFAULT_PORTFOLIO_CHANGE_POINTS_GRANULARITY,
    DEFAULT_PORTFOLIO_HISTORY_GRANULARITY, DEFAULT_PORTFOLIO_INCLUDE_PRICES,
};
pub use preflight::{run_preflight, PreflightOptions};
pub use spending::{spending_report, SpendingReportOptions};
pub use sync::{
    chase_login, schwab_login, sync_all, sync_all_if_stale, sync_backfill_metadata,
    sync_connection, sync_connection_if_stale, sync_prices, sync_recompact, sync_symlinks,
    SyncPricesScopeArg,
};
pub use types::{
    AccountOutput, AllOutput, AssetInfoOutput, BalanceOutput, ChangePointsOutput, ConnectionOutput,
    HistoryOutput, HistoryPoint, HistorySummary, PriceHistoryFailure, PriceHistoryOutput,
    PriceHistoryScopeOutput, PriceHistoryStats, PriceSourceOutput, SpendingBreakdownEntryOutput,
    SpendingOutput, SpendingPeriodOutput, SpendingScopeOutput, TransactionAnnotationOutput,
    TransactionOutput,
};

fn maybe_auto_commit(config: &ResolvedConfig, action: &str) {
    let committer = GitAutoCommitter::new(
        config.data_dir.clone(),
        config.git.auto_commit,
        config.git.auto_push,
    );
    committer.maybe_commit(action);
}
