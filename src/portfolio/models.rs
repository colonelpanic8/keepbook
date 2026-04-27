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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    pub account_id: String,
    pub account_name: String,
    pub connection_name: String,
    /// Value in base currency. None if any asset lacks price data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_in_base: Option<String>,
}
