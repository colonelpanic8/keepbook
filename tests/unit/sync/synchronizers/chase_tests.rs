use super::*;
use crate::sync::chase::api::{
    ActivityAccount, ChaseActivity, EnrichedMerchant, MerchantDetails, RawMerchantDetails,
};

#[test]
fn chase_account_kind_detects_credit_cards_and_mortgages() {
    assert_eq!(
        chase_account_kind("CreditCard", "Card"),
        ChaseAccountKind::CreditCard
    );
    assert_eq!(
        chase_account_kind("HomeLending", "Mortgage"),
        ChaseAccountKind::Mortgage
    );
    assert_eq!(
        chase_account_kind("Deposit", "Checking"),
        ChaseAccountKind::Other
    );
}

#[test]
fn liability_balance_amount_negates_amount_owed_and_preserves_overpayments() {
    assert_eq!(liability_balance_amount(250.0), "-250");
    assert_eq!(liability_balance_amount(-25.5), "25.5");
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
        additional_fields: Map::new(),
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

#[test]
fn chase_foreign_card_purchase_uses_settlement_currency_and_preserves_source_fields() {
    let activity: ChaseActivity = serde_json::from_value(serde_json::json!({
        "transactionStatusCode": "Posted",
        "transactionAmount": 56.14,
        "transactionDate": "2026-09-19",
        "transactionPostDate": "2026-09-21",
        "sorTransactionIdentifier": "346262130627027",
        "creditDebitCode": "D",
        "currencyCode": "392",
        "originalTransactionAmount": 8230,
        "originalCurrencyCode": "392"
    }))
    .unwrap();
    let connection_id = Id::from_string("conn-1");
    let tx = chase_activity_to_transaction(&activity, &connection_id, 123).unwrap();

    assert_eq!(tx.amount, "-56.14");
    assert_eq!(tx.asset, Asset::currency("USD"));
    assert_eq!(
        tx.id,
        chase_activity_to_transaction_id(&connection_id, 123, &activity)
    );
    assert_eq!(tx.synchronizer_data["chase_currency_code"], "392");
    assert_eq!(tx.synchronizer_data["originalTransactionAmount"], 8230);
    assert_eq!(tx.synchronizer_data["originalCurrencyCode"], "392");
}

#[test]
fn chase_domestic_card_purchase_uses_usd_asset() {
    let activity: ChaseActivity = serde_json::from_value(serde_json::json!({
        "transactionStatusCode": "Posted",
        "transactionAmount": 12.34,
        "transactionDate": "2026-02-15",
        "sorTransactionIdentifier": "domestic-1",
        "creditDebitCode": "D",
        "currencyCode": "840"
    }))
    .unwrap();
    let tx = chase_activity_to_transaction(&activity, &Id::from_string("conn-1"), 123).unwrap();

    assert_eq!(tx.amount, "-12.34");
    assert_eq!(tx.asset, Asset::currency("USD"));
    assert_eq!(tx.synchronizer_data["chase_currency_code"], "840");
}

fn stored_account(chase_account_id: i64, mask: &str, category_type: &str) -> Account {
    let mut account = Account::new_with(
        Id::from_external(&format!("chase:conn-1:{chase_account_id}")),
        Utc::now(),
        format!("Chase ({mask})"),
        Id::from_string("conn-1"),
    );
    account.synchronizer_data = serde_json::json!({
        "chase_account_id": chase_account_id,
        "mask": mask,
        "category_type": category_type,
        "account_type": "BAC",
    });
    account
}

fn stored_card(chase_account_id: i64, mask: &str) -> Account {
    stored_account(chase_account_id, mask, "CARD")
}

fn cards(stored: &[Account], listed: &[ActivityAccount]) -> CardsByMask {
    CardsByMask::new(&Id::from_string("conn-1"), stored, listed)
}

fn listed_card(chase_account_id: i64, mask: &str) -> ActivityAccount {
    ActivityAccount {
        id: chase_account_id,
        mask: mask.to_string(),
        nickname: String::new(),
        category_type: "CARD".to_string(),
        account_type: "SAPPHIRE".to_string(),
    }
}

fn feed_card(chase_account_id: i64, mask: &str) -> CardAccount {
    CardAccount {
        account_id: Id::from_external(&format!("chase:conn-1:{chase_account_id}")),
        chase_account_id,
        mask: mask.to_string(),
    }
}

fn card_activity(sor_id: &str, last4: Option<&str>) -> ChaseActivity {
    serde_json::from_value(serde_json::json!({
        "transactionStatusCode": "Posted",
        "transactionAmount": 42.5,
        "transactionDate": "2026-10-01",
        "transactionPostDate": "2026-10-02",
        "sorTransactionIdentifier": sor_id,
        "creditDebitCode": "D",
        "last4CardNumber": last4,
        "digitalAccountIdentifier": 1339069761_i64,
    }))
    .unwrap()
}

#[test]
fn old_card_activity_on_successor_feed_routes_to_predecessor_with_its_original_id() {
    let routing = cards(
        &[
            stored_card(886859518, "6395"),
            stored_card(1339069761, "9194"),
        ],
        &[listed_card(1339069761, "9194")],
    );
    let feed = feed_card(1339069761, "9194");
    let activity = card_activity("sor-old", Some(" 6395 "));

    let target = routing.route(&activity, &feed);
    assert_eq!(target, &feed_card(886859518, "6395"));

    let tx = chase_activity_to_transaction(
        &activity,
        &Id::from_string("conn-1"),
        target.chase_account_id,
    )
    .unwrap();
    assert_eq!(tx.id, Id::from_external("chase:conn-1:886859518:sor-old"));
    assert_eq!(tx.synchronizer_data["chase_account_id"], 886859518);
    assert_eq!(
        tx.synchronizer_data["digital_account_identifier"],
        1339069761
    );
}

#[test]
fn successor_or_unknown_card_activity_stays_on_feed_account() {
    let routing = cards(
        &[stored_card(886859518, "6395")],
        &[listed_card(1339069761, "9194")],
    );
    let feed = feed_card(1339069761, "9194");

    for last4 in [Some("9194"), Some("1111"), Some("  "), None] {
        assert_eq!(
            routing.route(&card_activity("sor-1", last4), &feed),
            &feed,
            "last4 {last4:?}"
        );
    }
}

#[test]
fn old_card_activity_keeps_its_owner_when_the_old_card_is_listed_again() {
    let routing = cards(
        &[
            stored_card(886859518, "6395"),
            stored_card(1339069761, "9194"),
        ],
        &[
            listed_card(886859518, "6395"),
            listed_card(1339069761, "9194"),
        ],
    );

    assert_eq!(
        routing.route(
            &card_activity("sor-old", Some("6395")),
            &feed_card(1339069761, "9194")
        ),
        &feed_card(886859518, "6395")
    );
}

#[test]
fn non_card_and_ambiguous_masks_are_never_routing_targets() {
    let routing = cards(
        &[
            stored_account(42, "1111", "MORTGAGE"),
            stored_card(886859518, "6395"),
            stored_account(43, "6395", "MORTGAGE"),
            stored_card(777, "2222"),
            stored_card(888, "2222"),
        ],
        &[listed_card(1339069761, "9194")],
    );
    let feed = feed_card(1339069761, "9194");

    assert_eq!(
        routing.route(&card_activity("sor-1", Some("1111")), &feed),
        &feed
    );
    assert_eq!(
        routing.route(&card_activity("sor-2", Some("2222")), &feed),
        &feed
    );
    assert_eq!(
        routing.route(&card_activity("sor-3", Some("6395")), &feed),
        &feed_card(886859518, "6395")
    );
}
