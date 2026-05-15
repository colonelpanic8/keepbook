use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

fn activity(id: &str) -> ChaseActivity {
    ChaseActivity {
        transaction_status_code: "P".to_string(),
        transaction_amount: 1.23,
        transaction_date: "2026-02-01".to_string(),
        transaction_post_date: None,
        sor_transaction_identifier: Some(id.to_string()),
        derived_unique_transaction_identifier: None,
        transaction_reference_number: None,
        credit_debit_code: "D".to_string(),
        etu_standard_transaction_type_name: None,
        etu_standard_transaction_type_group_name: None,
        etu_standard_expense_category_code: None,
        currency_code: Some("USD".to_string()),
        merchant_details: None,
        last4_card_number: None,
        digital_account_identifier: None,
    }
}

#[test]
fn signed_amount_normalizes_by_credit_debit_code() {
    let mut a = activity("a1");
    a.transaction_amount = -12.34;
    a.credit_debit_code = "C".to_string();
    assert!((a.signed_amount() - 12.34).abs() < 1e-9);

    a.credit_debit_code = "D".to_string();
    assert!((a.signed_amount() + 12.34).abs() < 1e-9);

    a.transaction_amount = 12.34;
    a.credit_debit_code = "C".to_string();
    assert!((a.signed_amount() - 12.34).abs() < 1e-9);

    a.credit_debit_code = "D".to_string();
    assert!((a.signed_amount() + 12.34).abs() < 1e-9);
}

fn resp(acts: Vec<ChaseActivity>, more: bool, key: Option<&str>) -> TransactionsResponse {
    TransactionsResponse {
        total_posted_transaction_count: None,
        posted_transaction_count: None,
        pending_authorization_count: None,
        activities: acts,
        more_records_indicator: more,
        pagination_contextual_text: key.map(|s| s.to_string()),
        last_sort_field_value_text: None,
        as_of_date: None,
    }
}

#[tokio::test]
async fn pagination_stops_when_more_records_false() -> Result<()> {
    let pages = vec![
        resp(vec![activity("a1"), activity("a2")], true, Some("k1")),
        resp(vec![activity("a3")], false, None),
    ];
    let q: Arc<Mutex<VecDeque<TransactionsResponse>>> = Arc::new(Mutex::new(VecDeque::from(pages)));
    let out = get_all_card_transactions_paginated("test", 2, 100, move |_key| {
        let q = q.clone();
        async move { Ok(q.lock().unwrap().pop_front().unwrap()) }
    })
    .await?;
    assert_eq!(out.len(), 3);
    Ok(())
}

#[tokio::test]
async fn pagination_stops_on_repeated_key() -> Result<()> {
    let pages = vec![
        resp(vec![activity("a1")], true, Some("k1")),
        resp(vec![activity("a2")], true, Some("k1")),
        // Would be infinite if we kept going.
        resp(vec![activity("a3")], true, Some("k1")),
    ];
    let q: Arc<Mutex<VecDeque<TransactionsResponse>>> = Arc::new(Mutex::new(VecDeque::from(pages)));
    let out = get_all_card_transactions_paginated("test", 1, 100, move |_key| {
        let q = q.clone();
        async move { Ok(q.lock().unwrap().pop_front().unwrap()) }
    })
    .await?;
    assert_eq!(out.len(), 2);
    Ok(())
}

#[tokio::test]
async fn pagination_truncates_to_max_transactions() -> Result<()> {
    let pages = vec![
        resp(vec![activity("a1"), activity("a2")], true, Some("k1")),
        resp(vec![activity("a3"), activity("a4")], true, Some("k2")),
    ];
    let q: Arc<Mutex<VecDeque<TransactionsResponse>>> = Arc::new(Mutex::new(VecDeque::from(pages)));
    let out = get_all_card_transactions_paginated("test", 2, 3, move |_key| {
        let q = q.clone();
        async move { Ok(q.lock().unwrap().pop_front().unwrap()) }
    })
    .await?;
    assert_eq!(out.len(), 3);
    Ok(())
}
