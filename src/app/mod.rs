mod config;
mod list;
mod mutations;
mod portfolio;
mod sync;
mod types;

use crate::config::ResolvedConfig;
use crate::sync::{AutoCommitter, GitAutoCommitter};

pub use config::config_output;
pub use list::{
    list_accounts, list_all, list_balances, list_connections, list_price_sources, list_transactions,
};
pub use mutations::{
    add_account, add_account_with, add_connection, add_connection_with, parse_asset,
    remove_connection, set_balance,
};
pub use portfolio::{
    fetch_historical_prices, portfolio_change_points, portfolio_history, portfolio_snapshot,
    PriceHistoryRequest,
};
pub use sync::{
    chase_login, schwab_login, sync_all, sync_all_if_stale, sync_connection,
    sync_connection_if_stale, sync_prices, sync_symlinks, SyncPricesScopeArg,
};
pub use types::{
    AccountOutput, AllOutput, AssetInfoOutput, BalanceOutput, ChangePointsOutput, ConnectionOutput,
    HistoryOutput, HistoryPoint, HistorySummary, PriceHistoryFailure, PriceHistoryOutput,
    PriceHistoryScopeOutput, PriceHistoryStats, PriceSourceOutput, TransactionOutput,
};

fn maybe_auto_commit(config: &ResolvedConfig, action: &str) {
    let committer = GitAutoCommitter::new(
        config.data_dir.clone(),
        config.git.auto_commit,
        config.git.auto_push,
    );
    committer.maybe_commit(action);
}
