mod config;
mod graph;
mod ignore_rules;
#[cfg(feature = "sync")]
mod import;
mod list;
mod mutations;
mod portfolio;
mod preflight;
mod spending;
#[cfg(feature = "sync")]
mod sync;
mod types;
mod value;

use crate::config::ResolvedConfig;

pub use config::config_output;
pub use graph::{portfolio_graph, PortfolioGraphOptions, PortfolioGraphOutput};
#[cfg(feature = "sync")]
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
    latent_capital_gains_tax_history, portfolio_change_points, portfolio_history,
    portfolio_history_for_accounts, portfolio_recent_history, portfolio_snapshot,
    portfolio_tax_impact, resolve_portfolio_history_selection, PortfolioHistorySelection,
    PriceHistoryRequest, DEFAULT_PORTFOLIO_CHANGE_POINTS_GRANULARITY,
    DEFAULT_PORTFOLIO_HISTORY_GRANULARITY, DEFAULT_PORTFOLIO_INCLUDE_PRICES,
};
pub use preflight::{run_preflight, PreflightOptions};
pub use spending::{spending_report, SpendingReportOptions};
#[cfg(feature = "sync")]
pub use sync::{
    chase_login, schwab_login, sync_all, sync_all_if_stale, sync_backfill_metadata,
    sync_connection, sync_connection_if_stale, sync_prices, sync_recompact, sync_symlinks,
    SyncPricesScopeArg,
};
pub use types::{
    AccountOutput, AllOutput, AssetInfoOutput, BalanceOutput, ChangePointsOutput, ConnectionOutput,
    HistoryOutput, HistoryPoint, HistorySummary, PriceHistoryFailure, PriceHistoryOutput,
    PriceHistoryScopeOutput, PriceHistoryStats, PriceSourceOutput, SpendingBreakdownEntryOutput,
    SpendingOutput, SpendingPeriodOutput, SpendingScopeOutput, TaxImpactGraphOutput,
    TaxImpactOutput, TaxImpactPoint, TransactionAnnotationOutput, TransactionOutput,
};

fn maybe_auto_commit(config: &ResolvedConfig, action: &str) {
    if !config.git.auto_commit {
        return;
    }

    #[cfg(feature = "git")]
    match crate::git::try_auto_commit(&config.data_dir, action, config.git.auto_push) {
        Ok(crate::git::AutoCommitOutcome::Committed) => {
            tracing::info!("Git auto-commit completed");
        }
        Ok(crate::git::AutoCommitOutcome::SkippedNoChanges) => {
            tracing::debug!("Git auto-commit skipped: no changes");
        }
        Ok(crate::git::AutoCommitOutcome::SkippedNotRepo { reason }) => {
            tracing::warn!("Git auto-commit skipped: {reason}");
        }
        Err(error) => {
            tracing::warn!("Git auto-commit failed: {error:#}");
        }
    }

    #[cfg(not(feature = "git"))]
    {
        let _ = action;
        tracing::warn!("Git auto-commit skipped: keepbook was built without git support");
    }
}

pub fn maybe_push_after_sync(config: &ResolvedConfig, enabled: bool) {
    if !enabled {
        return;
    }

    #[cfg(feature = "git")]
    match crate::git::try_push_remote(&config.data_dir) {
        Ok(crate::git::PushRemoteOutcome::Pushed) => {
            tracing::info!("Git push after sync completed");
        }
        Ok(crate::git::PushRemoteOutcome::SkippedNotRepo { reason }) => {
            tracing::warn!("Git push after sync skipped: {reason}");
        }
        Err(error) => {
            tracing::warn!("Git push after sync failed: {error:#}");
        }
    }

    #[cfg(not(feature = "git"))]
    tracing::warn!("Git push after sync skipped: keepbook was built without git support");
}
