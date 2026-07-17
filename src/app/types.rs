use chrono::NaiveDate;
use serde::Serialize;

use crate::models::{Asset, TransactionStandardizedMetadata};

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
    pub exclude_from_portfolio: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_display: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_basis: Option<String>,
    pub value_in_reporting_currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_in_reporting_currency_display: Option<String>,
    pub reporting_currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reporting_currency_symbol: Option<String>,
    pub timestamp: String,
}

/// JSON output for transactions
#[derive(Serialize)]
pub struct TransactionOutput {
    pub id: String,
    pub account_id: String,
    pub account_name: String,
    pub timestamp: String,
    pub description: String,
    pub amount: String,
    pub asset: serde_json::Value,
    pub status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subtags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotation: Option<TransactionAnnotationOutput>,
    #[serde(skip_serializing)]
    pub standardized_metadata: Option<TransactionStandardizedMetadata>,
}

/// Options for recurring transaction detection.
#[derive(Debug, Clone)]
pub struct RecurringTransactionsOptions {
    pub start: Option<String>,
    pub end: Option<String>,
    pub include_ignored: bool,
    pub include_possible: bool,
    pub min_confidence: f64,
}

/// Summary of the amount pattern for a recurring transaction candidate.
#[derive(Serialize)]
pub struct RecurringTransactionAmountOutput {
    pub typical: String,
    pub min: String,
    pub max: String,
    pub asset: serde_json::Value,
}

/// One transaction supporting a recurring transaction candidate.
#[derive(Serialize)]
pub struct RecurringTransactionOccurrenceOutput {
    pub id: String,
    pub account_id: String,
    pub account_name: String,
    pub date: String,
    pub description: String,
    pub amount: String,
}

/// A detected recurring transaction candidate.
#[derive(Serialize)]
pub struct RecurringTransactionOutput {
    pub name: String,
    pub normalized_name: String,
    pub status: String,
    pub cadence: String,
    /// Canonical interval length used for projections, in days.
    pub estimated_interval_days: String,
    /// Positive estimated cost for one occurrence.
    pub estimated_recurring_cost: String,
    /// Positive projected cost over one year.
    pub estimated_annual_cost: String,
    pub confidence: String,
    pub cadence_score: String,
    pub occurrence_count: usize,
    pub first_seen: String,
    pub last_seen: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_expected: Option<String>,
    pub amount: RecurringTransactionAmountOutput,
    pub reason_codes: Vec<String>,
    pub transactions: Vec<RecurringTransactionOccurrenceOutput>,
}

/// A persisted user decision for a recurring transaction candidate.
#[derive(Serialize)]
pub struct RecurringTransactionReviewOutput {
    pub candidate_key: String,
    pub updated_at: String,
    pub status: String,
    pub name: String,
    pub normalized_name: String,
    pub cadence: String,
    pub amount_typical: String,
    pub asset: serde_json::Value,
    pub transactions: Vec<RecurringTransactionReviewOccurrenceOutput>,
}

#[derive(Serialize)]
pub struct RecurringTransactionReviewOccurrenceOutput {
    pub account_id: String,
    pub transaction_id: String,
}

/// Materialized transaction annotation state.
#[derive(Serialize)]
pub struct TransactionAnnotationOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_spending: Option<bool>,
}

#[derive(Serialize)]
pub struct ProposedTransactionEditOutput {
    pub id: String,
    pub account_id: String,
    pub account_name: String,
    pub transaction_id: String,
    pub transaction_description: String,
    pub transaction_timestamp: String,
    pub transaction_amount: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
    pub patch: TransactionAnnotationPatchOutput,
}

#[derive(Serialize)]
pub struct TransactionAnnotationPatchOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Option<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtags: Option<Option<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_date: Option<Option<String>>,
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
    pub period_alignment: String,
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
    pub prospective_capital_gains_tax: Option<String>,
    pub percentage_change_from_previous: Option<String>,
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

/// Metadata for a series in a stacked portfolio contribution graph.
#[derive(Serialize)]
pub struct StackedHistorySeries {
    pub key: String,
    pub label: String,
    pub series_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<serde_json::Value>,
}

/// A component value for a date in a stacked portfolio contribution graph.
#[derive(Serialize)]
pub struct StackedHistoryComponent {
    pub series_key: String,
    pub value: String,
}

/// A single date in a stacked portfolio contribution graph.
#[derive(Serialize)]
pub struct StackedHistoryPoint {
    pub timestamp: String,
    pub date: String,
    pub total_value: String,
    pub components: Vec<StackedHistoryComponent>,
}

/// Output for stacked portfolio contribution history.
#[derive(Serialize)]
pub struct StackedHistoryOutput {
    pub currency: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub granularity: String,
    pub series: Vec<StackedHistorySeries>,
    pub points: Vec<StackedHistoryPoint>,
}

/// Output for the portfolio assets command: per-asset breakdown with
/// day/week/month/year changes.
#[derive(Serialize)]
pub struct AssetBreakdownOutput {
    pub as_of_date: NaiveDate,
    pub currency: String,
    /// Sum of all rows' value_in_base; rows with missing values contribute 0.
    pub total_value: String,
    pub assets: Vec<AssetBreakdownEntry>,
}

/// One (asset, liability) row of the per-asset portfolio breakdown.
#[derive(Serialize)]
pub struct AssetBreakdownEntry {
    pub asset: Asset,
    pub asset_id: String,
    /// True when this row aggregates negative-amount holdings.
    pub liability: bool,
    pub total_amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_date: Option<NaiveDate>,
    /// Value in base currency. None if price data unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_in_base: Option<String>,
    pub changes: AssetChanges,
    pub holdings: Vec<AssetBreakdownHolding>,
}

/// A single account's contribution to an asset breakdown row.
#[derive(Serialize)]
pub struct AssetBreakdownHolding {
    pub account_id: String,
    pub account_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_name: Option<String>,
    pub amount: String,
    pub balance_date: NaiveDate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_in_base: Option<String>,
}

/// Changes for an asset breakdown row over standard trailing periods.
#[derive(Serialize)]
pub struct AssetChanges {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<AssetChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub week: Option<AssetChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub month: Option<AssetChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<AssetChange>,
}

/// Change of a row's base-currency value versus a past date.
#[derive(Serialize)]
pub struct AssetChange {
    pub absolute: String,
    /// Percentage change vs |past value|. None when the past value was zero
    /// or the row had no holdings at the past date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<String>,
}

/// Summary statistics for the history
#[derive(Serialize)]
pub struct HistorySummary {
    pub initial_value: String,
    pub final_value: String,
    pub absolute_change: String,
    pub percentage_change: String,
}

/// A single point in a latent-tax impact curve.
#[derive(Serialize)]
pub struct TaxImpactPoint {
    pub nominal_net_worth: String,
    pub tax_liability: String,
    pub after_tax_net_worth: String,
    pub equity_multiplier: String,
    pub equity_change_percent: String,
}

/// Output for portfolio tax-impact command.
#[derive(Serialize)]
pub struct TaxImpactOutput {
    pub currency: String,
    pub as_of_date: String,
    pub capital_gains_tax_rate: String,
    pub current_nominal_net_worth: String,
    pub current_tax_liability: String,
    pub current_after_tax_net_worth: String,
    pub points: Vec<TaxImpactPoint>,
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
