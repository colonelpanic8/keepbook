use super::*;
use crate::clock::FixedClock;
use crate::models::{FixedIdGenerator, Id};
use chrono::TimeZone;

#[test]
fn transaction_new_with_generator_is_deterministic() {
    let ids = FixedIdGenerator::new([Id::from_string("tx-1")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

    let tx = Transaction::new_with_generator(&ids, &clock, "-1", Asset::currency("USD"), "Test");

    assert_eq!(tx.id.as_str(), "tx-1");
    assert_eq!(tx.timestamp, clock.now());
}

#[test]
fn with_synchronizer_data_derives_standardized_metadata_for_chase_fields() {
    let tx = Transaction::new("-10", Asset::currency("USD"), "Coffee").with_synchronizer_data(
        serde_json::json!({
            "merchant_dba_name": "Coffee Shop",
            "merchant_category_code": "5814",
            "merchant_category_name": "Fast Food",
            "etu_standard_transaction_type_group_name": "Purchases",
            "enriched_merchant_names": ["Blue Bottle Coffee"],
        }),
    );

    let md = tx.standardized_metadata.expect("expected metadata");
    assert_eq!(md.merchant_name.as_deref(), Some("Blue Bottle Coffee"));
    assert_eq!(md.merchant_category_code.as_deref(), Some("5814"));
    assert_eq!(md.merchant_category_label.as_deref(), Some("Fast Food"));
    assert_eq!(md.transaction_kind.as_deref(), Some("purchase"));
    assert_eq!(md.is_internal_transfer_hint, Some(false));
}

#[test]
fn backfill_standardized_metadata_populates_when_missing() {
    let tx = Transaction {
        id: Id::from_string("tx-1"),
        timestamp: Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap(),
        amount: "-10".to_string(),
        asset: Asset::currency("USD"),
        description: "Test".to_string(),
        status: TransactionStatus::Posted,
        synchronizer_data: serde_json::json!({
            "merchant_dba_name": "Coffee Shop",
            "merchant_category_code": "5814",
        }),
        standardized_metadata: None,
    }
    .backfill_standardized_metadata();

    let md = tx.standardized_metadata.expect("expected metadata");
    assert_eq!(md.merchant_name.as_deref(), Some("Coffee Shop"));
    assert_eq!(md.merchant_category_code.as_deref(), Some("5814"));
    assert_eq!(md.transaction_kind, None);
}

#[test]
fn with_synchronizer_data_uses_expense_category_code_when_label_missing() {
    let tx = Transaction::new("-10", Asset::currency("USD"), "Coffee").with_synchronizer_data(
        serde_json::json!({
            "etu_standard_expense_category_code": "FOOD_AND_DRINK",
        }),
    );

    let md = tx.standardized_metadata.expect("expected metadata");
    assert_eq!(
        md.merchant_category_label.as_deref(),
        Some("Food And Drink")
    );
}

#[test]
fn backfill_standardized_metadata_merges_missing_fields_when_present() {
    let tx = Transaction {
        id: Id::from_string("tx-1"),
        timestamp: Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap(),
        amount: "-10".to_string(),
        asset: Asset::currency("USD"),
        description: "Test".to_string(),
        status: TransactionStatus::Posted,
        synchronizer_data: serde_json::json!({
            "merchant_category_code": "5814",
            "etu_standard_expense_category_code": "FOOD_AND_DRINK",
        }),
        standardized_metadata: Some(TransactionStandardizedMetadata {
            merchant_name: Some("Existing Merchant".to_string()),
            merchant_category_code: None,
            merchant_category_label: None,
            transaction_kind: None,
            is_internal_transfer_hint: None,
        }),
    }
    .backfill_standardized_metadata();

    let md = tx.standardized_metadata.expect("expected metadata");
    assert_eq!(md.merchant_name.as_deref(), Some("Existing Merchant"));
    assert_eq!(md.merchant_category_code.as_deref(), Some("5814"));
    assert_eq!(
        md.merchant_category_label.as_deref(),
        Some("Food And Drink")
    );
}
