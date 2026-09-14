use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct RecurringTransaction {
    pub(crate) candidate_key: String,
    pub(crate) review_status: String,
    pub(crate) name: String,
    pub(crate) normalized_name: String,
    pub(crate) status: String,
    pub(crate) cadence: String,
    #[serde(default)]
    pub(crate) estimated_interval_days: String,
    #[serde(default)]
    pub(crate) estimated_recurring_cost: String,
    #[serde(default)]
    pub(crate) estimated_annual_cost: String,
    pub(crate) confidence: String,
    pub(crate) cadence_score: String,
    pub(crate) occurrence_count: usize,
    pub(crate) first_seen: String,
    pub(crate) last_seen: String,
    #[serde(default)]
    pub(crate) next_expected: Option<String>,
    pub(crate) amount: RecurringTransactionAmount,
    pub(crate) reason_codes: Vec<String>,
    pub(crate) transactions: Vec<RecurringTransactionOccurrence>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct RecurringTransactionAmount {
    pub(crate) typical: String,
    pub(crate) min: String,
    pub(crate) max: String,
    pub(crate) asset: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct RecurringTransactionOccurrence {
    pub(crate) id: String,
    pub(crate) account_id: String,
    pub(crate) account_name: String,
    pub(crate) date: String,
    pub(crate) description: String,
    pub(crate) amount: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct RecurringTransactionReviewInput {
    pub(crate) status: String,
    pub(crate) candidate: RecurringTransaction,
}
