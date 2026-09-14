use serde::Deserialize;

use super::{FilteringSettings, HistoryDefaults};

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Overview {
    pub(crate) config_path: String,
    pub(crate) data_dir: String,
    pub(crate) reporting_currency: String,
    pub(crate) history_defaults: HistoryDefaults,
    #[serde(default)]
    pub(crate) filtering: FilteringSettings,
    pub(crate) connections: Vec<Connection>,
    pub(crate) accounts: Vec<Account>,
    #[serde(default)]
    pub(crate) account_totals: AccountTotals,
    pub(crate) balances: Vec<Balance>,
    pub(crate) snapshot: PortfolioSnapshot,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub(crate) struct AccountTotals {
    pub(crate) account_count: usize,
    pub(crate) active_account_count: usize,
    pub(crate) excluded_account_count: usize,
    pub(crate) virtual_account_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Connection {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) synchronizer: String,
    pub(crate) status: String,
    pub(crate) account_count: usize,
    #[serde(default)]
    pub(crate) active_account_count: usize,
    #[serde(default)]
    pub(crate) excluded_account_count: usize,
    pub(crate) last_sync: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Account {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) connection_id: String,
    pub(crate) tags: Vec<String>,
    pub(crate) active: bool,
    #[serde(default)]
    pub(crate) exclude_from_portfolio: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Balance {
    pub(crate) account_id: String,
    pub(crate) asset: serde_json::Value,
    pub(crate) amount: String,
    pub(crate) value_in_reporting_currency: Option<String>,
    pub(crate) reporting_currency: String,
    pub(crate) timestamp: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct PortfolioSnapshot {
    pub(crate) as_of_date: String,
    pub(crate) currency: String,
    pub(crate) total_value: String,
    #[serde(default)]
    pub(crate) by_account: Vec<AccountSummary>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct TraySnapshot {
    pub(crate) total_label: String,
    pub(crate) as_of_date: String,
    pub(crate) history_lines: Vec<String>,
    pub(crate) portfolio_breakdown_lines: Vec<String>,
    pub(crate) spending_lines: Vec<String>,
    pub(crate) transaction_lines: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct AccountSummary {
    pub(crate) account_id: String,
    pub(crate) account_name: String,
    pub(crate) connection_name: String,
    pub(crate) value_in_base: Option<String>,
}
