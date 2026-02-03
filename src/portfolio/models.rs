// src/portfolio/models.rs
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::models::Asset;

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
    pub grouping: Grouping,
    pub include_detail: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    pub as_of_date: NaiveDate,
    pub currency: String,
    pub total_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_asset: Option<Vec<AssetSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_account: Option<Vec<AccountSummary>>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holdings: Option<Vec<AccountHolding>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountHolding {
    pub account_id: String,
    pub account_name: String,
    pub amount: String,
    pub balance_date: NaiveDate,
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
