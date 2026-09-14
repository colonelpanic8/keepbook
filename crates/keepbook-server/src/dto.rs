use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use keepbook::app::ReviewedRecurringTransactionOutput;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct HealthOutput {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct ConfigOutput {
    pub config_path: String,
    pub data_dir: String,
    pub reporting_currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reporting_currency_symbol: Option<String>,
    pub history_defaults: HistoryDefaultsOutput,
    pub filtering: FilteringOutput,
}

#[derive(Debug, Serialize)]
pub struct HistoryDefaultsOutput {
    pub portfolio_granularity: String,
    pub change_points_granularity: String,
    pub include_prices: bool,
    pub graph_range: String,
    pub graph_granularity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRemoteSettings {
    pub host: String,
    pub repo: String,
    pub branch: String,
    pub ssh_user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,
}

impl Default for GitRemoteSettings {
    fn default() -> Self {
        Self {
            host: "github.com".to_string(),
            repo: "colonelpanic8/keepbook-data".to_string(),
            branch: "master".to_string(),
            ssh_user: "git".to_string(),
            ssh_key_path: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct GitSettingsOutput {
    pub config_path: String,
    pub data_dir: String,
    pub git: GitRemoteSettings,
    pub repo_state: GitRepoState,
}

#[derive(Debug, Deserialize)]
pub struct GitSettingsInput {
    pub data_dir: String,
    pub host: String,
    pub repo: String,
    pub branch: String,
    pub ssh_user: String,
    #[serde(default)]
    pub ssh_key_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApplicationSettingsOutput {
    pub config_path: String,
    pub start_minimized_to_tray: bool,
    pub window_decorations: String,
}

#[derive(Debug, Deserialize)]
pub struct ApplicationSettingsInput {
    pub start_minimized_to_tray: bool,
    pub window_decorations: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RepositoryRegistryOutput {
    pub config_path: String,
    pub device_config_path: String,
    pub active_repository: Option<String>,
    pub repositories: Vec<RepositoryOutput>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RepositoryOutput {
    pub id: String,
    pub name: String,
    pub path: String,
    pub config_path: String,
    pub remote: String,
    pub branch: String,
    pub active: bool,
    pub cloned: bool,
    pub commit: Option<String>,
    pub managed: bool,
}

#[derive(Debug, Deserialize)]
pub struct AddRepositoryInput {
    #[serde(default)]
    pub name: String,
    pub path: String,
    pub remote: String,
    #[serde(default = "default_repository_branch")]
    pub branch: String,
}

#[derive(Debug, Deserialize)]
pub struct GitSyncInput {
    pub data_dir: String,
    pub host: String,
    pub repo: String,
    pub branch: String,
    pub ssh_user: String,
    pub private_key_pem: String,
    #[serde(default)]
    pub save_settings: bool,
}

#[derive(Clone, Default)]
pub struct GitSyncCancelToken {
    cancelled: Arc<AtomicBool>,
}

impl GitSyncCancelToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            anyhow::bail!("Git sync cancelled");
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct GitSyncOutput {
    pub ok: bool,
    pub data_dir: String,
    pub remote_url: String,
    pub branch: String,
}

#[derive(Debug, Deserialize)]
pub struct SyncConnectionsInput {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub if_stale: bool,
    #[serde(default)]
    pub full_transactions: bool,
}

#[derive(Debug, Deserialize)]
pub struct SyncPricesInput {
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub quote_staleness_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitRepoState {
    pub cloned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FilteringOutput {
    pub latent_capital_gains_tax: LatentCapitalGainsTaxFilterOutput,
}

#[derive(Debug, Serialize)]
pub struct LatentCapitalGainsTaxFilterOutput {
    pub configured_enabled: bool,
    pub effective_enabled: bool,
    pub override_enabled: Option<bool>,
    pub rate_configured: bool,
    pub account_name: String,
}

#[derive(Debug, Serialize)]
pub struct OverviewOutput {
    pub config_path: String,
    pub data_dir: String,
    pub reporting_currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reporting_currency_symbol: Option<String>,
    pub history_defaults: HistoryDefaultsOutput,
    pub filtering: FilteringOutput,
    pub connections: serde_json::Value,
    pub accounts: serde_json::Value,
    pub account_totals: serde_json::Value,
    pub balances: serde_json::Value,
    pub snapshot: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct TraySnapshotOutput {
    pub total_label: String,
    pub as_of_date: String,
    pub history_lines: Vec<String>,
    pub portfolio_breakdown_lines: Vec<String>,
    pub spending_lines: Vec<String>,
    pub transaction_lines: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionQuery {
    pub start: Option<String>,
    pub end: Option<String>,
    pub tz: Option<String>,
    #[serde(default)]
    pub sort_by_amount: bool,
    #[serde(default)]
    pub include_ignored: bool,
}

#[derive(Debug, Deserialize)]
pub struct RecurringTransactionsQuery {
    pub start: Option<String>,
    pub end: Option<String>,
    #[serde(default)]
    pub include_ignored: bool,
    #[serde(default)]
    pub include_possible: bool,
    pub min_confidence: Option<f64>,
    #[serde(default)]
    pub include_dismissed: bool,
}

#[derive(Debug, Deserialize)]
pub struct RecurringTransactionReviewInput {
    pub status: String,
    pub candidate: ReviewedRecurringTransactionOutput,
}

#[derive(Debug, Deserialize)]
pub struct TransactionTagTargetInput {
    pub account_id: String,
    pub transaction_id: String,
}

#[derive(Debug, Deserialize)]
pub struct TransactionTagsBatchInput {
    #[serde(default)]
    pub transactions: Vec<TransactionTagTargetInput>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub clear_tags: bool,
}

#[derive(Debug, Deserialize)]
pub struct TransactionIgnoreBatchInput {
    #[serde(default)]
    pub transactions: Vec<TransactionTagTargetInput>,
    #[serde(default)]
    pub ignore: bool,
}

#[derive(Debug, Deserialize)]
pub struct TransactionSubtagsBatchInput {
    #[serde(default)]
    pub transactions: Vec<TransactionTagTargetInput>,
    #[serde(default)]
    pub subtags: Vec<String>,
    #[serde(default)]
    pub clear_subtags: bool,
}

#[derive(Debug, Deserialize)]
pub struct TransactionEffectiveDateInput {
    pub account_id: String,
    pub transaction_id: String,
    #[serde(default)]
    pub effective_date: Option<String>,
    #[serde(default)]
    pub clear_effective_date: bool,
}

#[derive(Debug, Deserialize)]
pub struct SpendingQuery {
    pub currency: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub period: Option<String>,
    pub period_alignment: Option<String>,
    pub tz: Option<String>,
    pub week_start: Option<String>,
    pub bucket_days: Option<u64>,
    pub account: Option<String>,
    pub connection: Option<String>,
    pub status: Option<String>,
    pub direction: Option<String>,
    pub group_by: Option<String>,
    pub top: Option<usize>,
    pub lookback_days: Option<u32>,
    #[serde(default)]
    pub include_noncurrency: bool,
    #[serde(default)]
    pub include_empty: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct ProposedTransactionEditsQuery {
    #[serde(default)]
    pub include_decided: bool,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub currency: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub granularity: Option<String>,
    pub include_prices: Option<bool>,
    /// Ask for a `current` point valued at request time on top of the range's
    /// change points.
    pub include_current: Option<bool>,
    pub include_latent_capital_gains_tax: Option<bool>,
    pub account_portfolio_overrides: Option<String>,
    pub account: Option<String>,
    pub connection: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssetsQuery {
    pub date: Option<String>,
    pub account_portfolio_overrides: Option<String>,
    #[serde(default)]
    pub include_amount_changes: bool,
}

#[derive(Debug, Deserialize)]
pub struct OverviewQuery {
    pub history_start: Option<String>,
    pub history_end: Option<String>,
    pub history_granularity: Option<String>,
    pub include_prices: Option<bool>,
    pub include_latent_capital_gains_tax: Option<bool>,
    pub account_portfolio_overrides: Option<String>,
    #[serde(default)]
    pub include_history: bool,
}

fn default_repository_branch() -> String {
    "master".to_string()
}

#[cfg(test)]
#[path = "../tests/unit/dto_tests.rs"]
mod dto_tests;
