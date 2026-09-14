use serde::Deserialize;

use super::Transaction;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct SpendingOutput {
    pub(crate) currency: String,
    pub(crate) tz: String,
    pub(crate) start_date: String,
    pub(crate) end_date: String,
    #[serde(default)]
    pub(crate) period: String,
    pub(crate) total: String,
    pub(crate) transaction_count: usize,
    pub(crate) periods: Vec<SpendingPeriod>,
    pub(crate) skipped_transaction_count: usize,
    pub(crate) missing_price_transaction_count: usize,
    pub(crate) missing_fx_transaction_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct SpendingPeriod {
    pub(crate) start_date: String,
    pub(crate) end_date: String,
    pub(crate) total: String,
    pub(crate) transaction_count: usize,
    #[serde(default)]
    pub(crate) breakdown: Vec<SpendingBreakdownEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct SpendingBreakdownEntry {
    pub(crate) key: String,
    pub(crate) total: String,
    pub(crate) transaction_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct SpendingDashboardData {
    pub(crate) spending: SpendingOutput,
    pub(crate) spending_over_time: SpendingOutput,
    pub(crate) exact_match_spending: SpendingOutput,
    pub(crate) close_match_spending: SpendingOutput,
    pub(crate) transactions: Vec<Transaction>,
}
