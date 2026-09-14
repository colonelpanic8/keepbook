// src/portfolio/models.rs
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{Asset, Id};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grouping {
    Asset,
    Account,
    #[default]
    Both,
}

#[derive(Debug, Clone)]
pub struct PortfolioQuery {
    pub as_of_date: NaiveDate,
    /// Optional instant to value at, for points within a day.
    ///
    /// `None` means the end of `as_of_date`: the latest balance recorded on or
    /// before that date, valued with the latest price for it. When set, only
    /// balances recorded at or before this instant count, so a morning point
    /// cannot pick up an afternoon balance change. Among price and FX readings
    /// dated this instant's day, one recorded by it is preferred over one
    /// recorded after it. Readings for earlier dates, and a day whose readings
    /// were all recorded later, are used as for a date query, since a reading's
    /// timestamp is when keepbook stored it rather than a market time.
    pub as_of_timestamp: Option<DateTime<Utc>>,
    pub currency: String,
    /// If set, values denominated in `currency` are rounded to this many
    /// decimal places before being rendered as strings.
    pub currency_decimals: Option<u32>,
    pub grouping: Grouping,
    pub include_detail: bool,
    /// Optional capital gains tax rate as a decimal fraction (for example,
    /// 0.238 for 23.8%). Applied only to positive unrealized gains with known
    /// cost basis.
    pub capital_gains_tax_rate: Option<rust_decimal::Decimal>,
    /// Optional scenario that changes equity valuations before totals,
    /// unrealized gains, and prospective tax are calculated.
    pub equity_valuation_adjustment: Option<EquityValuationAdjustment>,
    /// Restrict valuation to these accounts. Empty means all non-excluded
    /// accounts in the portfolio.
    pub account_ids: Vec<Id>,
}

#[derive(Debug, Clone)]
pub enum EquityValuationAdjustment {
    /// Uniform percentage change to equity valuations. For example, -20 means
    /// a 20% downturn and +10 means a 10% increase.
    PercentChange(rust_decimal::Decimal),
    /// Uniformly scale equity valuations so the portfolio total before any
    /// virtual tax-liability account equals this amount.
    TargetPreTaxTotalValue(rust_decimal::Decimal),
}

/// Why an asset could not be valued.
///
/// A total only sums the assets that could be valued, so an operational failure
/// would otherwise look the same as a holding nobody has priced yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValuationIssueReason {
    /// No price is recorded for the asset at this date.
    MissingPrice,
    /// No FX rate is recorded for the conversion at this date.
    MissingFxRate,
    /// Looking the price up failed.
    PriceLookupFailed,
    /// Looking the FX rate up failed.
    FxLookupFailed,
}

/// An asset left out of a total, and why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValuationIssue {
    pub asset: Asset,
    pub reason: ValuationIssueReason,
    /// The underlying failure, for the lookup-failed reasons.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    pub as_of_date: NaiveDate,
    pub currency: String,
    pub total_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cost_basis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_unrealized_gain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prospective_capital_gains_tax: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valuation_scenario: Option<PortfolioValuationScenario>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_asset: Option<Vec<AssetSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_account: Option<Vec<AccountSummary>>,
    /// Assets that could not be valued and are therefore missing from
    /// `total_value`. Omitted when everything was valued.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub valuation_issues: Vec<ValuationIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioValuationScenario {
    pub equity_multiplier: String,
    pub equity_change_percent: String,
    pub pre_tax_total_value: String,
    pub equity_value_before: String,
    pub equity_value_after: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_pre_tax_total_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetSummary {
    pub asset: Asset,
    pub total_amount: String,
    /// Date of the most recent balance contributing to this amount.
    pub amount_date: NaiveDate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_date: Option<NaiveDate>,
    /// Exact timestamp when the price was fetched/recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_timestamp: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_date: Option<NaiveDate>,
    /// Value in base currency. None if price data unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_in_base: Option<String>,
    /// Sum of known cost basis for holdings of this asset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_basis: Option<String>,
    /// Unrealized gain for holdings with known cost basis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unrealized_gain: Option<String>,
    /// Estimated tax on positive unrealized gains when a tax rate is supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prospective_capital_gains_tax: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holdings: Option<Vec<AccountHolding>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountHolding {
    pub account_id: String,
    pub account_name: String,
    pub amount: String,
    pub balance_date: NaiveDate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_basis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unrealized_gain: Option<String>,
}

/// A single row in a per-asset portfolio breakdown. Rows are keyed by
/// (normalized asset, liability): positive holdings of an asset aggregate into
/// one row and negative holdings (debts) into a separate liability row.
#[derive(Debug, Clone)]
pub struct AssetBreakdownRow {
    pub asset: Asset,
    /// True when this row aggregates negative-amount holdings.
    pub liability: bool,
    pub total_amount: rust_decimal::Decimal,
    /// Value in target currency. None if price/FX data unavailable.
    pub value_in_base: Option<rust_decimal::Decimal>,
    pub price: Option<String>,
    pub price_date: Option<NaiveDate>,
    /// Exact timestamp when the price used for this row was fetched/recorded.
    pub price_timestamp: Option<DateTime<Utc>>,
    /// Most recent balance snapshot that checked this asset's amount.
    pub amount_last_checked_at: Option<DateTime<Utc>>,
    /// Most recent balance snapshot where this asset's aggregate amount changed.
    pub amount_last_changed_at: Option<DateTime<Utc>>,
    pub fx_rate: Option<String>,
    pub fx_date: Option<NaiveDate>,
    /// Why `value_in_base` is absent, when it is.
    pub value_issue: Option<ValuationIssue>,
    pub holdings: Vec<AssetBreakdownAccountHolding>,
}

/// A single account's contribution to an asset breakdown row.
#[derive(Debug, Clone)]
pub struct AssetBreakdownAccountHolding {
    pub account_id: String,
    pub account_name: String,
    pub connection_name: Option<String>,
    pub amount: rust_decimal::Decimal,
    pub balance_date: NaiveDate,
    /// Value in target currency. None if price/FX data unavailable.
    pub value_in_base: Option<rust_decimal::Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    pub account_id: String,
    pub account_name: String,
    pub connection_name: String,
    /// Value in base currency. None if any asset lacks price data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_in_base: Option<String>,
}
