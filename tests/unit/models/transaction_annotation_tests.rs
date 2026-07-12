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
        ignore_spending: None,
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
        ignore_spending: None,
    };
    clear_note.apply_to(&mut ann);
    assert_eq!(ann.note, None);
}

#[test]
fn patch_ignore_spending_tristate_apply_and_is_empty() {
    let tx_id = Id::from_string("tx-1");
    let mut ann = TransactionAnnotation::new(tx_id.clone());
    assert!(ann.is_empty());
    assert!(!ann.ignores_spending());

    let set_ignore = TransactionAnnotationPatch {
        transaction_id: tx_id.clone(),
        timestamp: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap(),
        description: None,
        note: None,
        tags: None,
        subtags: None,
        effective_date: None,
        ignore_spending: Some(Some(true)),
    };
    set_ignore.apply_to(&mut ann);
    assert_eq!(ann.ignore_spending, Some(true));
    assert!(!ann.is_empty());
    assert!(ann.ignores_spending());

    let clear_ignore = TransactionAnnotationPatch {
        transaction_id: tx_id,
        timestamp: Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 1).unwrap(),
        description: None,
        note: None,
        tags: None,
        subtags: None,
        effective_date: None,
        ignore_spending: Some(None),
    };
    clear_ignore.apply_to(&mut ann);
    assert_eq!(ann.ignore_spending, None);
    assert!(ann.is_empty());
    assert!(!ann.ignores_spending());
}

#[test]
fn ignore_spending_patch_serde_round_trip() {
    // Set: JSON true -> Some(Some(true)), serializes back to true.
    let patch: TransactionAnnotationPatch = serde_json::from_str(
        r#"{"transaction_id":"tx-1","timestamp":"2024-02-01T00:00:00Z","ignore_spending":true}"#,
    )
    .unwrap();
    assert_eq!(patch.ignore_spending, Some(Some(true)));
    let json = serde_json::to_value(&patch).unwrap();
    assert_eq!(json["ignore_spending"], serde_json::json!(true));

    let round_trip: TransactionAnnotationPatch = serde_json::from_value(json).unwrap();
    assert_eq!(round_trip, patch);

    // Clear: JSON null -> Some(None), serializes back to null.
    let patch: TransactionAnnotationPatch = serde_json::from_str(
        r#"{"transaction_id":"tx-1","timestamp":"2024-02-01T00:00:00Z","ignore_spending":null}"#,
    )
    .unwrap();
    assert_eq!(patch.ignore_spending, Some(None));
    let json = serde_json::to_value(&patch).unwrap();
    assert_eq!(json["ignore_spending"], serde_json::Value::Null);

    // Unset: absent field -> None, omitted on serialization.
    let patch: TransactionAnnotationPatch =
        serde_json::from_str(r#"{"transaction_id":"tx-1","timestamp":"2024-02-01T00:00:00Z"}"#)
            .unwrap();
    assert_eq!(patch.ignore_spending, None);
    let json = serde_json::to_value(&patch).unwrap();
    assert!(json.get("ignore_spending").is_none());
}

#[test]
fn annotation_ignores_spending_via_flag_or_magic_tag() {
    let mut ann = TransactionAnnotation::new(Id::from_string("tx-1"));
    assert!(!ann.ignores_spending());

    // Explicit false adds nothing (no force-include override).
    ann.ignore_spending = Some(false);
    assert!(!ann.ignores_spending());

    ann.ignore_spending = Some(true);
    assert!(ann.ignores_spending());

    // Legacy magic tags still work.
    for tag in ["ignore_spending", "ignore-spending", "IGNORE:SPENDING"] {
        let mut tagged = TransactionAnnotation::new(Id::from_string("tx-1"));
        tagged.tags = Some(vec![tag.to_string()]);
        assert!(tagged.ignores_spending(), "tag {tag} should ignore");
    }
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
