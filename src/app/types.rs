use serde::Serialize;

use crate::models::Asset;

/// JSON output for connections
#[derive(Serialize)]
pub struct ConnectionOutput {
    pub id: String,
    pub name: String,
    pub synchronizer: String,
    pub status: String,
    pub account_count: usize,
    pub last_sync: Option<String>,
}

/// JSON output for accounts
#[derive(Serialize)]
pub struct AccountOutput {
    pub id: String,
    pub name: String,
    pub connection_id: String,
    pub tags: Vec<String>,
    pub active: bool,
}

/// JSON output for price sources
#[derive(Serialize)]
pub struct PriceSourceOutput {
    pub name: String,
    #[serde(rename = "type")]
    pub source_type: String,
    pub enabled: bool,
    pub priority: u32,
    pub has_credentials: bool,
}

/// JSON output for balances
#[derive(Serialize)]
pub struct BalanceOutput {
    pub account_id: String,
    pub asset: serde_json::Value,
    pub amount: String,
    pub value_in_reporting_currency: Option<String>,
    pub reporting_currency: String,
    pub timestamp: String,
}

/// JSON output for transactions
#[derive(Serialize)]
pub struct TransactionOutput {
    pub id: String,
    pub account_id: String,
    pub timestamp: String,
    pub description: String,
    pub amount: String,
    pub asset: serde_json::Value,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotation: Option<TransactionAnnotationOutput>,
}

/// Materialized transaction annotation state.
#[derive(Serialize)]
pub struct TransactionAnnotationOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Scope output for spending report.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SpendingScopeOutput {
    Portfolio,
    Connection { id: String, name: String },
    Account { id: String, name: String },
}

#[derive(Serialize)]
pub struct SpendingBreakdownEntryOutput {
    pub key: String,
    pub total: String,
    pub transaction_count: usize,
}

#[derive(Serialize)]
pub struct SpendingPeriodOutput {
    pub start_date: String,
    pub end_date: String,
    pub total: String,
    pub transaction_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub breakdown: Vec<SpendingBreakdownEntryOutput>,
}

#[derive(Serialize)]
pub struct SpendingOutput {
    pub scope: SpendingScopeOutput,
    pub currency: String,
    pub tz: String,
    pub start_date: String,
    pub end_date: String,
    pub period: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub week_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket_days: Option<u32>,
    pub direction: String,
    pub status: String,
    pub group_by: String,
    pub total: String,
    pub transaction_count: usize,
    pub periods: Vec<SpendingPeriodOutput>,
    pub skipped_transaction_count: usize,
    pub missing_price_transaction_count: usize,
    pub missing_fx_transaction_count: usize,
}

/// Combined output for list all
#[derive(Serialize)]
pub struct AllOutput {
    pub connections: Vec<ConnectionOutput>,
    pub accounts: Vec<AccountOutput>,
    pub price_sources: Vec<PriceSourceOutput>,
    pub balances: Vec<BalanceOutput>,
}

/// A single point in the net worth history
#[derive(Serialize)]
pub struct HistoryPoint {
    pub timestamp: String,
    pub date: String,
    pub total_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_triggers: Option<Vec<String>>,
}

/// Output for portfolio history command
#[derive(Serialize)]
pub struct HistoryOutput {
    pub currency: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub granularity: String,
    pub points: Vec<HistoryPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<HistorySummary>,
}

/// Summary statistics for the history
#[derive(Serialize)]
pub struct HistorySummary {
    pub initial_value: String,
    pub final_value: String,
    pub absolute_change: String,
    pub percentage_change: String,
}

/// Output for portfolio change-points command
#[derive(Serialize)]
pub struct ChangePointsOutput {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub granularity: String,
    pub include_prices: bool,
    pub points: Vec<crate::portfolio::ChangePoint>,
}

/// Scope output for market data history fetch
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PriceHistoryScopeOutput {
    Portfolio,
    Connection { id: String, name: String },
    Account { id: String, name: String },
}

/// Asset info output for market data history fetch
#[derive(Serialize)]
pub struct AssetInfoOutput {
    pub asset: Asset,
    pub asset_id: String,
}

/// Summary stats for market data history fetch
#[derive(Default, Serialize)]
pub struct PriceHistoryStats {
    pub attempted: usize,
    pub existing: usize,
    pub fetched: usize,
    pub lookback: usize,
    pub missing: usize,
}

/// Failure details for market data history fetch (sampled)
#[derive(Serialize)]
pub struct PriceHistoryFailure {
    pub kind: String,
    pub date: String,
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<Asset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
}

/// Output for market data history fetch
#[derive(Serialize)]
pub struct PriceHistoryOutput {
    pub scope: PriceHistoryScopeOutput,
    pub currency: String,
    pub interval: String,
    pub start_date: String,
    pub end_date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earliest_balance_date: Option<String>,
    pub days: usize,
    pub points: usize,
    pub assets: Vec<AssetInfoOutput>,
    pub prices: PriceHistoryStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx: Option<PriceHistoryStats>,
    pub failure_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<PriceHistoryFailure>,
}
