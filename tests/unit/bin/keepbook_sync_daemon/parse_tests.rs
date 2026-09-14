use super::*;

#[test]
fn parse_sync_counts_handles_mixed_results() {
    let value = serde_json::json!({
        "total": 4,
        "results": [
            {"success": true},
            {"success": true, "skipped": true, "reason": "manual"},
            {"success": true, "skipped": true, "reason": "not stale"},
            {"success": false, "error": "boom"}
        ]
    });

    let counts = parse_sync_counts(&value);
    assert_eq!(counts.total, 4);
    assert_eq!(counts.synced, 1);
    assert_eq!(counts.skipped_manual, 1);
    assert_eq!(counts.skipped_not_stale, 1);
    assert_eq!(counts.failed, 1);
}
