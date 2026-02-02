// src/portfolio/models.rs
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::models::Asset;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grouping {
    Asset,
    Account,
    Both,
}

impl Default for Grouping {
    fn default() -> Self {
        Self::Both
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshMode {
    CachedOnly,
    IfStale,
    Force,
}

impl Default for RefreshMode {
    fn default() -> Self {
        Self::CachedOnly
    }
}

#[derive(Debug, Clone)]
pub struct PortfolioQuery {
    pub as_of_date: NaiveDate,
    pub currency: String,
    pub grouping: Grouping,
    pub include_detail: bool,
}

#[derive(Debug, Clone)]
pub struct RefreshPolicy {
    pub mode: RefreshMode,
    pub stale_threshold: std::time::Duration,
}

impl Default for RefreshPolicy {
    fn default() -> Self {
        Self {
            mode: RefreshMode::CachedOnly,
            stale_threshold: std::time::Duration::from_secs(24 * 60 * 60), // 1 day
        }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_date: Option<NaiveDate>,
    pub value_in_base: String,
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
    pub value_in_base: String,
}
