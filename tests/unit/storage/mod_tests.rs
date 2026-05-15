use super::dedupe_transactions_last_write_wins;
use crate::models::{Asset, Id, Transaction, TransactionStatus};
use chrono::TimeZone;

fn chase_tx(
    id: &str,
    stable_id: &str,
    sor_id: Option<&str>,
    derived_id: Option<&str>,
) -> Transaction {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "chase_account_id".to_string(),
        serde_json::Value::Number(123.into()),
    );
    obj.insert(
        "stable_id".to_string(),
        serde_json::Value::String(stable_id.to_string()),
    );
    if let Some(v) = sor_id {
        obj.insert(
            "sor_transaction_identifier".to_string(),
            serde_json::Value::String(v.to_string()),
        );
    }
    if let Some(v) = derived_id {
        obj.insert(
            "derived_unique_transaction_identifier".to_string(),
            serde_json::Value::String(v.to_string()),
        );
    }

    Transaction {
        id: Id::from_string(id),
        timestamp: chrono::Utc.with_ymd_and_hms(2026, 2, 20, 12, 0, 0).unwrap(),
        amount: "-10".to_string(),
        asset: Asset::currency("USD"),
        description: "Test".to_string(),
        status: TransactionStatus::Posted,
        synchronizer_data: serde_json::Value::Object(obj),
        standardized_metadata: None,
    }
    .backfill_standardized_metadata()
}

#[test]
fn dedupe_transactions_collapses_chase_alias_ids() {
    let old = chase_tx("tx-old", "202602151536556260124#20260124", None, None);
    let new_no_alias = chase_tx("tx-new", "466046216565116", None, None);
    let new_with_alias = chase_tx(
        "tx-new",
        "466046216565116",
        Some("466046216565116"),
        Some("202602151536556260124#20260124"),
    );

    let out = dedupe_transactions_last_write_wins(vec![old, new_no_alias, new_with_alias]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id.as_str(), "tx-new");
}
