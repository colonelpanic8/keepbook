use super::*;
use crate::sync::chase::api::{
    ActivityAccount, ChaseActivity, EnrichedMerchant, MerchantDetails, RawMerchantDetails,
};

fn activity_account(category_type: &str, account_type: &str) -> ActivityAccount {
    ActivityAccount {
        id: 123,
        mask: "1234".to_string(),
        nickname: "Test".to_string(),
        category_type: category_type.to_string(),
        account_type: account_type.to_string(),
    }
}

#[test]
fn chase_account_kind_detects_credit_cards_and_mortgages() {
    assert_eq!(
        chase_account_kind(&activity_account("CreditCard", "Card")),
        ChaseAccountKind::CreditCard
    );
    assert_eq!(
        chase_account_kind(&activity_account("HomeLending", "Mortgage")),
        ChaseAccountKind::Mortgage
    );
    assert_eq!(
        chase_account_kind(&activity_account("Deposit", "Checking")),
        ChaseAccountKind::Other
    );
}

#[test]
fn liability_balance_amount_negates_amount_owed_and_preserves_overpayments() {
    assert_eq!(liability_balance_amount(250.0), "-250");
    assert_eq!(liability_balance_amount(-25.5), "25.5");
}

#[test]
fn transient_execution_context_errors_are_retryable() {
    assert!(is_transient_execution_context_error(
        "Error -32000: Cannot find context with specified id"
    ));
    assert!(is_transient_execution_context_error(
        "Execution context was destroyed, most likely because of a navigation"
    ));
    assert!(is_transient_execution_context_error(
        "Inspected target navigated or closed"
    ));
    assert!(!is_transient_execution_context_error(
        "Error -32601: Method not found"
    ));
}

#[test]
fn chase_activity_to_transaction_persists_extended_metadata() {
    let activity = ChaseActivity {
        transaction_status_code: "Posted".to_string(),
        transaction_amount: 12.34,
        transaction_date: "2026-02-15".to_string(),
        transaction_post_date: Some("2026-02-16".to_string()),
        sor_transaction_identifier: Some("sor-123".to_string()),
        derived_unique_transaction_identifier: Some("derived-456".to_string()),
        transaction_reference_number: Some("ref-789".to_string()),
        credit_debit_code: "D".to_string(),
        etu_standard_transaction_type_name: Some("Card Purchase".to_string()),
        etu_standard_transaction_type_group_name: Some("Purchases".to_string()),
        etu_standard_expense_category_code: Some("FOOD_AND_DRINK".to_string()),
        currency_code: Some("USD".to_string()),
        merchant_details: Some(MerchantDetails {
            raw_merchant_details: Some(RawMerchantDetails {
                merchant_dba_name: Some("Coffee Shop".to_string()),
                merchant_city_name: Some("San Francisco".to_string()),
                merchant_state_code: Some("CA".to_string()),
                merchant_category_code: Some("5814".to_string()),
                merchant_category_name: Some("Fast Food".to_string()),
            }),
            enriched_merchants: vec![EnrichedMerchant {
                merchant_name: Some("Blue Bottle Coffee".to_string()),
                merchant_role_type_code: Some(101),
            }],
        }),
        last4_card_number: Some("1234".to_string()),
        digital_account_identifier: Some(987654321),
    };

    let tx = chase_activity_to_transaction(&activity, &Id::from_string("conn-1"), 123)
        .expect("expected transaction to parse");
    let data = tx
        .synchronizer_data
        .as_object()
        .expect("expected synchronizer_data object");

    assert_eq!(
        data.get("etu_standard_expense_category_code")
            .and_then(|v| v.as_str()),
        Some("FOOD_AND_DRINK")
    );
    assert_eq!(
        data.get("merchant_category_code").and_then(|v| v.as_str()),
        Some("5814")
    );
    assert_eq!(
        data.get("merchant_category_name").and_then(|v| v.as_str()),
        Some("Fast Food")
    );
    assert_eq!(
        data.get("merchant_dba_name").and_then(|v| v.as_str()),
        Some("Coffee Shop")
    );
    assert_eq!(
        data.get("enriched_merchant_names"),
        Some(&Value::Array(vec![Value::String(
            "Blue Bottle Coffee".to_string()
        )]))
    );
    assert_eq!(
        data.get("enriched_merchant_role_type_codes"),
        Some(&Value::Array(vec![Value::Number(101.into())]))
    );
    assert_eq!(
        data.get("digital_account_identifier")
            .and_then(|v| v.as_i64()),
        Some(987654321)
    );
}
