use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::AssetId;
use crate::models::Asset;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceKind {
    /// End-of-day closing price
    Close,
    /// Adjusted closing price (for splits/dividends)
    AdjClose,
    /// Real-time or delayed quote (intraday)
    Quote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FxRateKind {
    Close,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricePoint {
    pub asset_id: AssetId,
    pub as_of_date: NaiveDate,
    pub timestamp: DateTime<Utc>,
    pub price: String,
    pub quote_currency: String,
    pub kind: PriceKind,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxRatePoint {
    pub base: String,
    pub quote: String,
    pub as_of_date: NaiveDate,
    pub timestamp: DateTime<Utc>,
    pub rate: String,
    pub kind: FxRateKind,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRegistryEntry {
    pub id: AssetId,
    pub asset: Asset,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub provider_ids: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
}

impl AssetRegistryEntry {
    pub fn new(asset: Asset) -> Self {
        Self {
            id: AssetId::from_asset(&asset),
            asset,
            provider_ids: HashMap::new(),
            tz: None,
        }
    }
}
