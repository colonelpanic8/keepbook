use super::*;
use crate::models::Id;
use chrono::TimeZone;

#[test]
fn patch_tristate_semantics_apply() {
    let tx_id = Id::from_string("tx-1");
    let mut ann = TransactionAnnotation::new(tx_id.clone());

    let set_note = TransactionAnnotationPatch {
        transaction_id: tx_id.clone(),
        timestamp: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap(),
        description: None,
        note: Some(Some("hello".to_string())),
        tags: None,
        subtags: None,
        effective_date: None,
    };
    set_note.apply_to(&mut ann);
    assert_eq!(ann.note, Some("hello".to_string()));

    let clear_note = TransactionAnnotationPatch {
        transaction_id: tx_id,
        timestamp: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 1).unwrap(),
        description: None,
        note: Some(None),
        tags: None,
        subtags: None,
        effective_date: None,
    };
    clear_note.apply_to(&mut ann);
    assert_eq!(ann.note, None);
}

#[test]
fn patch_json_null_round_trips_as_explicit_clear() {
    let patch: TransactionAnnotationPatch = serde_json::from_str(
        r#"{"transaction_id":"tx-1","timestamp":"2024-02-01T00:00:00Z","description":null}"#,
    )
    .unwrap();
    assert_eq!(patch.description, Some(None));
    assert_eq!(patch.note, None);
}
