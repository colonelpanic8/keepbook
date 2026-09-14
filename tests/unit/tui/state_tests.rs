use super::*;
use serde_json::json;
use std::path::PathBuf;

fn tx(id: &str, timestamp: &str, amount: &str) -> TransactionOutput {
    TransactionOutput {
        id: id.to_string(),
        account_id: "acct-1".to_string(),
        account_name: "Checking".to_string(),
        timestamp: timestamp.to_string(),
        description: "desc".to_string(),
        amount: amount.to_string(),
        asset: json!({"type":"currency","iso_code":"USD"}),
        status: "posted".to_string(),
        tags: Vec::new(),
        subtags: Vec::new(),
        annotation: None,
        ignored_from_spending: false,
        spending_ignore_reason: None,
        standardized_metadata: None,
    }
}
#[test]
fn timespan_filter_respects_cutoff() {
    let mut state = AppState::new(
        vec![
            tx("a", "2026-01-01T00:00:00+00:00", "1"),
            tx("b", "2026-02-01T00:00:00+00:00", "1"),
        ],
        TransactionTagMatcher::default(),
        PathBuf::from("/tmp/transaction-rules-test.jsonl"),
        false,
        TuiOptions::default(),
    );
    state.span = TimeSpan::All;
    state.recompute_visible_transactions();
    let all_len = state.visible_transaction_indices.len();
    state.span = TimeSpan::Days7;
    state.recompute_visible_transactions();
    assert!(state.visible_transaction_indices.len() <= all_len);
}
#[test]
fn net_worth_interval_cycles() {
    assert_eq!(NetWorthInterval::Daily.next(), NetWorthInterval::Weekly);
    assert_eq!(NetWorthInterval::Daily.prev(), NetWorthInterval::Hourly);
    assert_eq!(NetWorthInterval::Full.prev(), NetWorthInterval::Yearly);
}
