use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct ProposedTransactionEdit {
    pub(crate) id: String,
    pub(crate) account_id: String,
    pub(crate) account_name: String,
    pub(crate) transaction_id: String,
    pub(crate) transaction_description: String,
    pub(crate) transaction_timestamp: String,
    pub(crate) transaction_amount: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) status: String,
    pub(crate) patch: ProposedTransactionEditPatch,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Default)]
pub(crate) struct ProposedTransactionEditPatch {
    pub(crate) description: Option<Option<String>>,
    pub(crate) note: Option<Option<String>>,
    pub(crate) tags: Option<Option<Vec<String>>>,
    pub(crate) subtags: Option<Option<Vec<String>>>,
    pub(crate) effective_date: Option<Option<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Transaction {
    pub(crate) id: String,
    pub(crate) account_id: String,
    pub(crate) account_name: String,
    pub(crate) timestamp: String,
    pub(crate) description: String,
    pub(crate) amount: String,
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    pub(crate) subtags: Vec<String>,
    #[serde(default)]
    pub(crate) annotation: Option<TransactionAnnotation>,
    #[serde(default)]
    pub(crate) ignored_from_spending: bool,
    /// Why the app layer leaves this transaction out of spending reports.
    /// Mirrors `keepbook::app::SpendingIgnoreReason` as its snake_case wire form.
    #[serde(default)]
    pub(crate) spending_ignore_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct TransactionAnnotation {
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) tags: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) subtags: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) effective_date: Option<String>,
    /// Explicit per-transaction ignore-from-spending annotation. Lets the UI
    /// distinguish "ignored via annotation" from "ignored via rule".
    #[serde(default)]
    pub(crate) ignore_spending: Option<bool>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct AiRuleTransactionInput {
    pub(crate) id: String,
    pub(crate) account_id: String,
    pub(crate) account_name: String,
    pub(crate) timestamp: String,
    pub(crate) description: String,
    pub(crate) amount: String,
    pub(crate) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subtag: Option<String>,
    pub(crate) ignored_from_spending: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct AiRuleSuggestionInput {
    pub(crate) prompt: String,
    pub(crate) transactions: Vec<AiRuleTransactionInput>,
    pub(crate) existing_tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct AiRuleSuggestionsOutput {
    pub(crate) model: String,
    pub(crate) selected_transaction_count: usize,
    pub(crate) suggestions: Vec<AiRuleToolCallOutput>,
    #[serde(default)]
    pub(crate) message: Option<String>,
    #[serde(default)]
    pub(crate) response_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct AiRuleToolCallOutput {
    pub(crate) name: String,
    pub(crate) arguments: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct TransactionTagTargetInput {
    pub(crate) account_id: String,
    pub(crate) transaction_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct SetTransactionTagsInput {
    pub(crate) transactions: Vec<TransactionTagTargetInput>,
    pub(crate) tags: Vec<String>,
    pub(crate) clear_tags: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct SetTransactionIgnoreInput {
    pub(crate) transactions: Vec<TransactionTagTargetInput>,
    pub(crate) ignore: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct SetTransactionEffectiveDateInput {
    pub(crate) account_id: String,
    pub(crate) transaction_id: String,
    pub(crate) effective_date: Option<String>,
    pub(crate) clear_effective_date: bool,
}
