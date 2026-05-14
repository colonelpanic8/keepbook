mod classification;
mod config;
mod ignore_rules;
#[cfg(feature = "sync")]
mod import;
mod list;
mod mutations;
mod portfolio;
mod preflight;
mod recurring;
mod spending;
#[cfg(feature = "sync")]
mod sync;
mod transaction_rules;
mod types;
mod value;

use crate::config::ResolvedConfig;

use anyhow::Context;

pub use config::config_output;
#[cfg(feature = "sync")]
pub use import::import_schwab_transactions;
pub use list::{
    list_accounts, list_all, list_balances, list_connections, list_price_sources, list_transactions,
};
pub use mutations::{
    add_account, add_account_with, add_connection, add_connection_with,
    approve_proposed_transaction_edit, list_proposed_transaction_edits, parse_asset,
    propose_transaction_edit, propose_transaction_edit_with, reject_proposed_transaction_edit,
    remove_connection, remove_proposed_transaction_edit, set_account_config, set_balance,
    set_transaction_annotation, set_transaction_subtags, set_transaction_tags,
};
pub use portfolio::{
    default_portfolio_change_points_granularity, default_portfolio_history_granularity,
    default_portfolio_include_prices, fetch_historical_prices, fill_prices_at_date,
    latent_capital_gains_tax_history, portfolio_change_points, portfolio_history,
    portfolio_history_for_accounts, portfolio_recent_history, portfolio_snapshot,
    portfolio_stacked_history, portfolio_tax_impact, resolve_portfolio_history_selection,
    PortfolioHistorySelection, PriceHistoryRequest, DEFAULT_PORTFOLIO_CHANGE_POINTS_GRANULARITY,
    DEFAULT_PORTFOLIO_HISTORY_GRANULARITY, DEFAULT_PORTFOLIO_INCLUDE_PRICES,
};
pub use preflight::{run_preflight, PreflightOptions};
pub use recurring::list_recurring_transactions;
pub use spending::{spending_report, SpendingReportOptions};
#[cfg(feature = "sync")]
pub use sync::{
    chase_login, schwab_login, sync_all, sync_all_if_stale, sync_backfill_metadata,
    sync_connection, sync_connection_if_stale, sync_prices, sync_recompact, sync_symlinks,
    SyncPricesScopeArg,
};
pub use transaction_rules::{
    add_transaction_rule, append_transaction_rule, apply_transaction_rules, list_transaction_rules,
    load_transaction_rules, transaction_rules_config_path, transaction_rules_path,
    ApplyTransactionRulesOptions, TransactionRule, TransactionRuleInput, TransactionRuleMatcher,
};
pub use types::{
    AccountOutput, AllOutput, AssetInfoOutput, BalanceOutput, ChangePointsOutput, ConnectionOutput,
    HistoryOutput, HistoryPoint, HistorySummary, PriceHistoryFailure, PriceHistoryOutput,
    PriceHistoryScopeOutput, PriceHistoryStats, PriceSourceOutput, ProposedTransactionEditOutput,
    RecurringTransactionAmountOutput, RecurringTransactionOccurrenceOutput,
    RecurringTransactionOutput, RecurringTransactionsOptions, SpendingBreakdownEntryOutput,
    SpendingOutput, SpendingPeriodOutput, SpendingScopeOutput, StackedHistoryComponent,
    StackedHistoryOutput, StackedHistoryPoint, StackedHistorySeries, TaxImpactOutput,
    TaxImpactPoint, TransactionAnnotationOutput, TransactionAnnotationPatchOutput, TransactionOutput,
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

pub fn maybe_push_after_sync(config: &ResolvedConfig, enabled: bool) -> anyhow::Result<()> {
    if !enabled {
        return Ok(());
    }

    #[cfg(feature = "git")]
    match crate::git::try_push_remote(&config.data_dir) {
        Ok(crate::git::PushRemoteOutcome::Pushed) => {
            tracing::info!("Git push after sync completed");
            Ok(())
        }
        Ok(crate::git::PushRemoteOutcome::SkippedNotRepo { reason }) => {
            anyhow::bail!("Git push after sync required but skipped: {reason}");
        }
        Err(error) => Err(error).context("Git push after sync failed"),
    }

    #[cfg(not(feature = "git"))]
    anyhow::bail!("Git push after sync required but keepbook was built without git support");
}
