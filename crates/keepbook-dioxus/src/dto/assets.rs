use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct AssetBreakdown {
    pub(crate) as_of_date: String,
    pub(crate) currency: String,
    #[serde(default)]
    pub(crate) change_mode: String,
    pub(crate) total_value: String,
    #[serde(default)]
    pub(crate) asset_count: usize,
    #[serde(default)]
    pub(crate) liability_count: usize,
    pub(crate) assets: Vec<AssetBreakdownEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct AssetBreakdownEntry {
    pub(crate) asset: serde_json::Value,
    pub(crate) asset_id: String,
    pub(crate) liability: bool,
    pub(crate) total_amount: String,
    #[serde(default)]
    pub(crate) price: Option<String>,
    #[serde(default)]
    pub(crate) price_date: Option<String>,
    #[serde(default)]
    pub(crate) price_updated_at: Option<String>,
    #[serde(default)]
    pub(crate) amount_last_checked_at: Option<String>,
    #[serde(default)]
    pub(crate) amount_last_changed_at: Option<String>,
    #[serde(default)]
    pub(crate) value_in_base: Option<String>,
    pub(crate) changes: AssetChanges,
    pub(crate) holdings: Vec<AssetBreakdownHolding>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub(crate) struct AssetChanges {
    #[serde(default)]
    pub(crate) day: Option<AssetChange>,
    #[serde(default)]
    pub(crate) week: Option<AssetChange>,
    #[serde(default)]
    pub(crate) month: Option<AssetChange>,
    #[serde(default)]
    pub(crate) year: Option<AssetChange>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct AssetChange {
    pub(crate) absolute: String,
    #[serde(default)]
    pub(crate) percentage: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct AssetBreakdownHolding {
    pub(crate) account_id: String,
    pub(crate) account_name: String,
    #[serde(default)]
    pub(crate) connection_name: Option<String>,
    pub(crate) amount: String,
    pub(crate) balance_date: String,
    #[serde(default)]
    pub(crate) value_in_base: Option<String>,
}
