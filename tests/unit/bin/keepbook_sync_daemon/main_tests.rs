use super::*;
use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind};

#[test]
fn compute_next_delay_without_jitter_is_constant() {
    let interval = Duration::from_secs(1800);
    let jitter = Duration::ZERO;
    let delay = compute_next_delay(interval, jitter);
    assert_eq!(delay, interval);
}

#[test]
fn compute_next_delay_with_jitter_stays_in_range() {
    let interval = Duration::from_secs(600);
    let jitter = Duration::from_secs(120);

    for _ in 0..100 {
        let delay = compute_next_delay(interval, jitter);
        assert!(delay >= Duration::from_secs(480));
        assert!(delay <= Duration::from_secs(720));
    }
}

#[test]
fn parse_nonzero_duration_rejects_zero() {
    assert!(parse_nonzero_duration_arg("0s").is_err());
    assert_eq!(
        parse_nonzero_duration_arg("30s").expect("duration should parse"),
        Duration::from_secs(30)
    );
}

#[test]
fn fs_event_filter_includes_state_mutations() {
    assert!(should_refresh_for_fs_event_kind(&EventKind::Any));
    assert!(should_refresh_for_fs_event_kind(&EventKind::Create(
        CreateKind::Any
    )));
    assert!(should_refresh_for_fs_event_kind(&EventKind::Modify(
        ModifyKind::Any
    )));
    assert!(should_refresh_for_fs_event_kind(&EventKind::Remove(
        RemoveKind::Any
    )));
}

#[test]
fn fs_event_filter_excludes_access_events() {
    assert!(!should_refresh_for_fs_event_kind(&EventKind::Access(
        AccessKind::Any
    )));
}

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
